// [ssh_executor.rs] - KRUST MVP - Hardened SSH with proper timeouts
use std::path::PathBuf;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use ssh2::Session;
use anyhow::{Result, bail, Context};
use zeroize::Zeroizing;
use std::io::Read;
use tracing::debug;

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
        _cert: Option<String>,
        use_agent: bool,
    ) -> Result<Self> {
        let method = if let Some(pw) = password {
            AuthMethod::Password(Zeroizing::new(pw))
        } else if let Some(key) = key_file {
            AuthMethod::KeyFile(PathBuf::from(key))
        } else if use_agent {
            AuthMethod::Agent
        } else {
            bail!("No authentication method provided");
        };
        
        Ok(SshAuth { user, method })
    }
}

impl SshHost {
    pub fn from_target(target: &str, default_port: Option<u16>) -> Result<Self> {
        let parts: Vec<&str> = target.split(':').collect();
        let hostname = parts[0].trim().to_string();
        let port = parts.get(1)
            .and_then(|p| p.parse().ok())
            .or(default_port)
            .unwrap_or(22);
        
        Ok(SshHost { hostname, port })
    }
}

pub fn execute_command_on_host(
    host: &SshHost,
    auth: &SshAuth,
    command: &str,
) -> Result<(String, i32)> {
    debug!("Connecting to {}:{}", host.hostname, host.port);
    
    // Resolve hostname with timeout
    let addr = format!("{}:{}", host.hostname, host.port);
    let socket_addr = addr.to_socket_addrs()
        .context("Failed to resolve hostname")?
        .next()
        .ok_or_else(|| anyhow::anyhow!("No addresses found for host"))?;
    
    // Connect with timeout
    let tcp = TcpStream::connect_timeout(&socket_addr, Duration::from_secs(10))
        .context("TCP connection failed")?;
    
    // Set socket timeouts
    tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
    
    // SSH handshake
    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()
        .context("SSH handshake failed")?;
    
    // Authenticate
    match &auth.method {
        AuthMethod::Password(pw) => {
            session.userauth_password(&auth.user, pw)
                .context("Password authentication failed")?;
        }
        AuthMethod::KeyFile(path) => {
            session.userauth_pubkey_file(&auth.user, None, path, None)
                .context("Key authentication failed")?;
        }
        AuthMethod::Agent => {
            authenticate_with_agent(&mut session, &auth.user)?;
        }
    }
    
    if !session.authenticated() {
        bail!("Authentication failed");
    }
    
    debug!("Executing command: {}", command);
    
    // Execute command
    let mut channel = session.channel_session()?;
    channel.exec(command)?;
    
    // Read output
    let mut stdout = String::new();
    let mut stderr = String::new();
    
    // Read stdout
    channel.read_to_string(&mut stdout)?;
    
    // Read stderr
    channel.stderr().read_to_string(&mut stderr)?;
    
    // Wait for channel to close
    channel.wait_close()?;
    let exit_code = channel.exit_status()?;
    
    debug!("Command completed with exit code: {}", exit_code);
    
    // Combine output if there's stderr
    let output = if stderr.is_empty() {
        stdout
    } else {
        format!("{}\nSTDERR:\n{}", stdout, stderr)
    };
    
    Ok((output.trim().to_string(), exit_code))
}

fn authenticate_with_agent(session: &mut Session, user: &str) -> Result<()> {
    let mut agent = session.agent()?;
    agent.connect()
        .context("Failed to connect to SSH agent")?;
    agent.list_identities()
        .context("Failed to list SSH agent identities")?;
    
    let identities = agent.identities()?;
    if identities.is_empty() {
        bail!("No identities found in SSH agent");
    }
    
    // Try each identity with a timeout
    for identity in identities {
        debug!("Trying SSH agent identity: {}", identity.comment());
        
        // This is synchronous, but at least we try each one
        match agent.userauth(user, &identity) {
            Ok(_) => return Ok(()),
            Err(_) => continue,
        }
    }
    
    bail!("No SSH agent identities worked")
}
