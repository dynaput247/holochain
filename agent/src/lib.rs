//! holochain_agent provides a library for managing holochain agent info, including identities, keys etc..

/// Object holding an Agent's identity.
#[derive(Clone, Debug, PartialEq)]
pub struct Identity {
    content: String,
}

impl Identity {
    pub fn new(content: String) -> Self {
        Identity { content }
    }
}

/// Object holding all Agent's data.
#[derive(Clone, Debug, PartialEq)]
pub struct Agent {
    identity: Identity,
}

impl Agent {
    pub fn new(id: Identity) -> Self {
        Agent { identity: id }
    }

    pub fn from_string(text: String) -> Self {
        Agent::new(Identity { content: text })
    }

    pub fn to_string(&self) -> String {
        self.identity.content.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn can_instantiate_agent() {
        let agent = Agent::new(Identity {
            content: "bob".to_string(),
        });
        assert_eq!(agent.identity.content, "bob".to_string());

        let agent = Agent::from_string("jane".to_string());
        assert_eq!(agent.identity.content, "jane".to_string());

        assert_eq!(agent.to_string(), "jane".to_string());
    }
}
