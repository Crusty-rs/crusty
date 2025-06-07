use std::path::PathBuf;
use std::net::TcpStream;
use std::time::Duration;
use ssh2::Session;
use anyhow::{Result, bail};
use zeroize::Zeroizing;
use std::io::Read;
use tracing::{info, debug}; // 👈 FIXED: Removed unused 'warn' and 'error'

#[derive(Debug)]
pub enum PasswordAuth {
    Password(Zeroizing<String>),
    KeyFile(PathBuf),
    Agent,
    Cert(PathBuf),
    None,
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
pub struct SshHost {
    pub hostname: String,
    pub port: u16,
}

#[derive(Debug)]
pub struct SshAuth {
    pub user: String,
    pub auth_method: PasswordAuth,
}

impl SshAuth {
    pub fn new(
        user: String, 
        password: Option<String>, 
        key_file: Option<String>, 
        cert_file: Option<String>,
        use_agent: bool
    ) -> Result<Self> {
        let auth_method = if let Some(pw) = password {
            PasswordAuth::Password(Zeroizing::new(pw))
        } else if let Some(cert) = cert_file {
            PasswordAuth::Cert(PathBuf::from(cert))
        } else if let Some(kf) = key_file {
            PasswordAuth::KeyFile(PathBuf::from(kf))
        } else if use_agent {
            PasswordAuth::Agent
        } else {
            PasswordAuth::None
        };

        Ok(SshAuth { user, auth_method })
    }
}

pub fn execute_command_on_host(
    host: &SshHost, 
    auth: &SshAuth, 
    command: &str, 
    timeout: Duration, 
    _retries: u8
) -> Result<String> {
    debug!(host = %host.hostname, command = %command, "Connecting to host");
    
    let tcp = TcpStream::connect(format!("{}:{}", host.hostname, host.port))?;
    tcp.set_read_timeout(Some(timeout))?;
    tcp.set_write_timeout(Some(timeout))?;

    let mut session = Session::new()?;
    session.set_tcp_stream(tcp);
    session.handshake()?;
    
    debug!(host = %host.hostname, "SSH handshake completed");

    match &auth.auth_method {
        PasswordAuth::Password(pw) => {
            debug!(host = %host.hostname, "Authenticating with password");
            session.userauth_password(&auth.user, pw)?;
        }
        PasswordAuth::KeyFile(path) => {
            debug!(host = %host.hostname, "Authenticating with key file");
            session.userauth_pubkey_file(&auth.user, None, path, None)?;
        }
        PasswordAuth::Cert(cert_path) => {
            debug!(host = %host.hostname, "Authenticating with certificate");
            session.userauth_pubkey_file(&auth.user, None, cert_path, None)?;
        }
        PasswordAuth::Agent => {
            debug!(host = %host.hostname, "Authenticating with SSH agent");
            let mut agent = session.agent()?;
            agent.connect()?;
            agent.list_identities()?;
            let identities = agent.identities()?;

            let mut success = false;
            for identity in identities {
                if agent.userauth(&auth.user, &identity).is_ok() {
                    success = true;
                    break;
                }
            }

            if !success {
                bail!("Agent authentication failed");
            }
        }
        PasswordAuth::None => {
            bail!("No authentication method provided");
        }
    }

    if !session.authenticated() {
        bail!("Authentication failed");
    }
    
    info!(host = %host.hostname, "Authentication successful, executing command");

    let mut channel = session.channel_session()?;
    channel.exec(command)?;

    let mut output = String::new();
    channel.read_to_string(&mut output)?;

    channel.wait_close()?;
    let exit_code = channel.exit_status()?;
    
    info!(
        host = %host.hostname,
        exit_code = exit_code,
        output_size = output.len(),
        "Command execution completed"
    );

    if exit_code != 0 {
        bail!("Remote command failed with exit code {}", exit_code);
    }

    Ok(output.trim().to_string())
}

impl SshHost {
    pub fn from_target(target: &str, default_port: Option<u16>) -> Result<Self> {
        let parts: Vec<&str> = target.split(':').collect();

        let hostname = parts[0].trim().to_string();
        let port = if parts.len() > 1 {
            parts[1].parse::<u16>().unwrap_or(22)
        } else {
            default_port.unwrap_or(22)
        };

        Ok(SshHost { hostname, port })
    }
}
