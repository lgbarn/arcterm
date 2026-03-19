//! Capability parsing and enforcement for WASM plugins.

use std::path::PathBuf;

/// A specific capability resource type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityResource {
    Terminal,
    Filesystem,
    Network,
    Keybinding,
}

/// A specific capability operation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CapabilityOperation {
    Read,
    Write,
    Connect,
    Register,
}

/// A parsed capability grant.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Capability {
    pub resource: CapabilityResource,
    pub operation: CapabilityOperation,
    pub target: Option<String>,
}

/// Error when a capability check fails.
#[derive(Debug, thiserror::Error)]
#[error("Plugin denied capability {resource}:{operation} — not granted")]
pub struct CapabilityDenied {
    pub resource: String,
    pub operation: String,
}

impl Capability {
    /// Parse a capability string like "fs:read:/home/user/projects".
    pub fn parse(s: &str) -> anyhow::Result<Self> {
        let parts: Vec<&str> = s.splitn(3, ':').collect();
        if parts.len() < 2 {
            anyhow::bail!(
                "Invalid capability format: '{}'. Expected '<resource>:<operation>[:<target>]'",
                s
            );
        }

        let (resource, operation) = match (parts[0], parts[1]) {
            ("terminal", "read") => (CapabilityResource::Terminal, CapabilityOperation::Read),
            ("terminal", "write") => (CapabilityResource::Terminal, CapabilityOperation::Write),
            ("fs", "read") => (CapabilityResource::Filesystem, CapabilityOperation::Read),
            ("fs", "write") => (CapabilityResource::Filesystem, CapabilityOperation::Write),
            ("net", "connect") => (CapabilityResource::Network, CapabilityOperation::Connect),
            ("keybinding", "register") => (
                CapabilityResource::Keybinding,
                CapabilityOperation::Register,
            ),
            _ => anyhow::bail!("Unknown capability: '{}:{}'", parts[0], parts[1]),
        };

        let target = parts.get(2).map(|s| s.to_string());

        // Validate that fs and net capabilities include a target
        match resource {
            CapabilityResource::Filesystem if target.is_none() => {
                anyhow::bail!(
                    "Filesystem capability requires a path target: 'fs:{}:<path>'",
                    parts[1]
                );
            }
            CapabilityResource::Network if target.is_none() => {
                anyhow::bail!(
                    "Network capability requires a host:port target: 'net:connect:<host>:<port>'"
                );
            }
            _ => {}
        }

        Ok(Capability {
            resource,
            operation,
            target,
        })
    }
}

/// A set of capabilities granted to a plugin.
#[derive(Debug, Clone, Default)]
pub struct CapabilitySet {
    capabilities: Vec<Capability>,
}

impl CapabilitySet {
    /// Create a new capability set from parsed capabilities.
    /// Always includes terminal:read as a default grant.
    pub fn new(mut caps: Vec<Capability>) -> Self {
        // Ensure terminal:read is always granted
        let has_terminal_read = caps.iter().any(|c| {
            c.resource == CapabilityResource::Terminal && c.operation == CapabilityOperation::Read
        });
        if !has_terminal_read {
            caps.push(Capability {
                resource: CapabilityResource::Terminal,
                operation: CapabilityOperation::Read,
                target: None,
            });
        }
        CapabilitySet { capabilities: caps }
    }

    /// Check if a capability is granted. Returns Ok(()) if allowed,
    /// Err(CapabilityDenied) if denied.
    pub fn check(&self, required: &Capability) -> Result<(), CapabilityDenied> {
        for cap in &self.capabilities {
            if cap.resource == required.resource && cap.operation == required.operation {
                // For filesystem: check path prefix with traversal protection
                if cap.resource == CapabilityResource::Filesystem {
                    if let (Some(granted_path), Some(requested_path)) =
                        (&cap.target, &required.target)
                    {
                        let requested = PathBuf::from(requested_path);

                        // SECURITY: Reject any path containing ".." components
                        // to prevent sandbox escape via path traversal
                        if requested.components().any(|c| {
                            matches!(c, std::path::Component::ParentDir)
                        }) {
                            log::warn!(
                                "Capability denied: path traversal detected in '{}'",
                                requested_path
                            );
                            continue;
                        }

                        let granted = PathBuf::from(granted_path);
                        if requested.starts_with(&granted) {
                            return Ok(());
                        }
                        continue;
                    }
                }
                // For network: check host:port match
                if cap.resource == CapabilityResource::Network {
                    if cap.target == required.target {
                        return Ok(());
                    }
                    continue;
                }
                // For terminal and keybinding: no target check needed
                return Ok(());
            }
        }

        Err(CapabilityDenied {
            resource: format!("{:?}", required.resource),
            operation: format!("{:?}", required.operation),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_terminal_read() {
        let cap = Capability::parse("terminal:read").unwrap();
        assert_eq!(cap.resource, CapabilityResource::Terminal);
        assert_eq!(cap.operation, CapabilityOperation::Read);
        assert_eq!(cap.target, None);
    }

    #[test]
    fn test_parse_fs_read() {
        let cap = Capability::parse("fs:read:/home/user").unwrap();
        assert_eq!(cap.resource, CapabilityResource::Filesystem);
        assert_eq!(cap.operation, CapabilityOperation::Read);
        assert_eq!(cap.target, Some("/home/user".to_string()));
    }

    #[test]
    fn test_parse_fs_without_path_fails() {
        assert!(Capability::parse("fs:read").is_err());
    }

    #[test]
    fn test_parse_net_connect() {
        let cap = Capability::parse("net:connect:api.example.com:443").unwrap();
        assert_eq!(cap.resource, CapabilityResource::Network);
        assert_eq!(cap.target, Some("api.example.com:443".to_string()));
    }

    #[test]
    fn test_capability_set_default_terminal_read() {
        let set = CapabilitySet::new(vec![]);
        let required = Capability {
            resource: CapabilityResource::Terminal,
            operation: CapabilityOperation::Read,
            target: None,
        };
        assert!(set.check(&required).is_ok());
    }

    #[test]
    fn test_capability_set_denies_fs_without_grant() {
        let set = CapabilitySet::new(vec![]);
        let required = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some("/etc/passwd".to_string()),
        };
        assert!(set.check(&required).is_err());
    }

    #[test]
    fn test_capability_set_fs_path_prefix() {
        let caps = vec![Capability::parse("fs:read:/home/user").unwrap()];
        let set = CapabilitySet::new(caps);

        // Within granted path: allowed
        let allowed = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some("/home/user/file.txt".to_string()),
        };
        assert!(set.check(&allowed).is_ok());

        // Outside granted path: denied
        let denied = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some("/etc/passwd".to_string()),
        };
        assert!(set.check(&denied).is_err());
    }

    #[test]
    fn test_path_traversal_blocked() {
        let caps = vec![Capability::parse("fs:read:/home/user").unwrap()];
        let set = CapabilitySet::new(caps);

        // Path traversal via ../ must be denied
        let traversal = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some("/home/user/../.ssh/id_rsa".to_string()),
        };
        assert!(set.check(&traversal).is_err());

        // Another traversal attempt
        let traversal2 = Capability {
            resource: CapabilityResource::Filesystem,
            operation: CapabilityOperation::Read,
            target: Some("/home/user/../../etc/passwd".to_string()),
        };
        assert!(set.check(&traversal2).is_err());
    }
}
