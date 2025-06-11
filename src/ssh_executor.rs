// [ssh_executor.rs] - KRUST - Production-Hardened SSH Executor
use std::path::PathBuf;
use std::net::{TcpStream, ToSocketAddrs, SocketAddr};
use std::time::Duration;
use ssh2::Session;
use anyhow::{Result, bail, Context};
use zeroize::Zeroizing;
use std::io::Read;
use tracing::{debug, trace};

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SshHost {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug)]
pub enum AuthMethod {
    Password(Zeroizing<String>),
    KeyFile(PathBuf),
    Agent,
}

#[derive(Debug)]
pub struct SshAuth {
    pub user: String,
    pub method: AuthMethod,
}

impl SshAuth {
    pub fn new(
        user: String,
        password: Option<String>,
        key_file: Option<String>,
        use_agent: bool,
    ) -> Result<Self> {
        // Smart auth selection: prefer key -> agent -> password
        let method = if let Some(key) = key_file {
            AuthMethod::KeyFile(PathBuf::from(key))
        } else if use_agent && password.is_none() {
            AuthMethod::Agent
        } else if let Some(pw) = password {
            AuthMethod::Password(Zeroizing::new(pw))
        } else {
            // Default to trying common key locations
            let default_keys = vec![
                dirs::home_dir().map(|h| h.join(".ssh/id_rsa")),
                dirs::home_dir().map(|h| h.join(".ssh/id_ed25519")),
                dirs::home_dir().map(|h| h.join(".ssh/id_ecdsa")),
            ];
            
            for key_path in default_keys.into_iter().flatten() {
                if key_path.exists() {
                    debug!("Using default key: {:?}", key_path);
                    return Ok(SshAuth {
                        user,
                        method: AuthMethod::KeyFile(key_path),
                    });
                }
            }
            
            // Fall back to agent if no keys found
            AuthMethod::Agent
        };
        
        Ok(SshAuth { user, method })
    }
}

impl SshHost {
    pub fn from_target(target: &str, default_port: Option<u16>) -> Result<Self> {
        let parts: Vec<&str> = target.split(':').collect();
        let hostname = parts[0].trim().to_string();
        
        if hostname.is_empty() {
            bail!("Empty hostname");
        }
        
        let port = parts.get(1)
            .and_then(|p| p.parse().ok())
            .or(default_port)
            .unwrap_or(22);
        
        if port == 0 {
            bail!("Invalid port: {}", port);
        }
        
        Ok(SshHost { hostname, port })
    }
}

pub fn execute_command_on_host(
    host: &SshHost,
    auth: &SshAuth,
    command: &str,
) -> Result<(String, i32)> {
    debug!("Connecting to {}:{}", host.hostname, host.port);
    
    // Resolve hostname with timeout and cache
    let addr = format!("{}:{}", host.hostname, host.port);
    let socket_addrs: Vec<SocketAddr> = addr.to_socket_addrs()
        .context("Failed to resolve hostname")?
        .collect();
    
    if socket_addrs.is_empty() {
        bail!("No addresses found for host");
    }
    
    // Try each resolved address
    let mut last_error = None;
    let mut tcp = None;
    
    for socket_addr in socket_addrs {
        trace!("Trying address: {}", socket_addr);
        match TcpStream::connect_timeout(&socket_addr, Duration::from_secs(10)) {
            Ok(stream) => {
                tcp = Some(stream);
                break;
            }
            Err(e) => {
                last_error = Some(e);
                continue;
            }
        }
    }
    
    let tcp = tcp.ok_or_else(|| {
        anyhow::anyhow!("TCP connection failed: {:?}", last_error)
    })?;
    
    // Configure TCP for production use
    tcp.set_nodelay(true)?; // Disable Nagle's algorithm for lower latency
    tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
    
    // SSH handshake with timeout
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.set_timeout(30_000); // 30 second timeout for SSH operations
    
    session.handshake()
        .context("SSH handshake failed")?;
    
    // Try authentication methods with fallback
    let mut auth_errors = Vec::new();
    
    match &auth.method {
        AuthMethod::KeyFile(path) => {
            trace!("Trying key authentication: {:?}", path);
            match session.userauth_pubkey_file(&auth.user, None, path, None) {
                Ok(_) => {},
                Err(e) => {
                    auth_errors.push(format!("Key auth failed: {}", e));
                    // Try agent as fallback
                    if let Err(e) = authenticate_with_agent(&mut session, &auth.user) {
                        auth_errors.push(format!("Agent fallback failed: {}", e));
                    }
                }
            }
        }
        AuthMethod::Agent => {
            trace!("Trying agent authentication");
            if let Err(e) = authenticate_with_agent(&mut session, &auth.user) {
                auth_errors.push(format!("Agent auth failed: {}", e));
            }
        }
        AuthMethod::Password(pw) => {
            trace!("Trying password authentication");
            if let Err(e) = session.userauth_password(&auth.user, pw) {
                auth_errors.push(format!("Password auth failed: {}", e));
            }
        }
    }
    
    if !session.authenticated() {
        bail!("Authentication failed: {}", auth_errors.join("; "));
    }
    
    debug!("Authenticated successfully, executing command");
    
    // Execute command with proper channel configuration
    let mut channel = session.channel_session()?;
    
    // Set channel environment if needed
    channel.handle_extended_data(ssh2::ExtendedData::Merge)?;
    
    // Execute the command
    channel.exec(command)?;
    
    // Read output efficiently
    let mut output = Vec::with_capacity(4096);
    channel.read_to_end(&mut output)?;
    
    // Ensure channel is closed and get exit status
    channel.wait_close()?;
    let exit_code = channel.exit_status()?;
    
    trace!("Command completed with exit code: {}", exit_code);
    
    // Convert output to string, handling invalid UTF-8 gracefully
    let output_string = String::from_utf8_lossy(&output).into_owned();
    
    Ok((output_string.trim_end().to_string(), exit_code))
}

fn authenticate_with_agent(session: &mut Session, user: &str) -> Result<()> {
    let mut agent = session.agent()?;
    
    // Connect to agent with error context
    agent.connect()
        .context("Failed to connect to SSH agent - is ssh-agent running?")?;
    
    agent.list_identities()
        .context("Failed to list SSH agent identities")?;
    
    let identities = agent.identities()?;
    if identities.is_empty() {
        bail!("No identities found in SSH agent - run ssh-add");
    }
    
    // Try each identity
    let mut errors = Vec::new();
    for identity in identities {
        trace!("Trying SSH agent identity: {}", identity.comment());
        
        match agent.userauth(user, &identity) {
            Ok(_) => return Ok(()),
            Err(e) => {
                errors.push(format!("{}: {}", identity.comment(), e));
                continue;
            }
        }
    }
    
    bail!("No SSH agent identities worked: {}", errors.join("; "))
}

// Helper to add dirs crate for home directory detection
mod dirs {
    use std::path::PathBuf;
    
    pub fn home_dir() -> Option<PathBuf> {
        #[cfg(unix)]
        {
            std::env::var_os("HOME").map(PathBuf::from)
        }
        #[cfg(windows)]
        {
            std::env::var_os("USERPROFILE").map(PathBuf::from)
        }
    }
}
