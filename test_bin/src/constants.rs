use serde_json::json;

use holochain_core_types::{cas::content::Address, hash::HashString};

// CONSTS
// TODO: make an array of agent_ids instead?
pub static ALEX_AGENT_ID: &'static str = "alex";
pub static BILLY_AGENT_ID: &'static str = "billy";
pub static CAMILLE_AGENT_ID: &'static str = "camille";

pub static META_CRUD_ATTRIBUTE: &'static str = "crud";
pub static META_LINK_ATTRIBUTE: &'static str = "link__yay";

lazy_static! {
    pub static ref DNA_ADDRESS: Address = HashString::from("DUMMY_DNA_ADDRESS");
    pub static ref ENTRY_ADDRESS_1: Address = HashString::from("entry_addr_1");
    pub static ref ENTRY_ADDRESS_2: Address = HashString::from("entry_addr_2");
    pub static ref ENTRY_ADDRESS_3: Address = HashString::from("entry_addr_3");
    pub static ref ENTRY_CONTENT_1: serde_json::Value = json!({"entry":{"content":"hello"}});
    pub static ref ENTRY_CONTENT_2: serde_json::Value = json!({"entry":{"content":"hello-2"}});
    pub static ref ENTRY_CONTENT_3: serde_json::Value = json!({"entry":{"content":"hello-3"}});
    // TODO: Meta content should be an Address instead
    pub static ref META_CRUD_CONTENT: serde_json::Value = json!("LIVE");
//    pub static ref META_LINK_CONTENT_1: serde_json::Value = json!({
//        "entry":{
//            "LinkAdd":{
//                "action_kind":"ADD",
//                "link":{
//                    "base":"sandwich--------------------------------------------------------------------------AAAEqzh28L",
//                    "target":"Qmb8DamUYDt3n2uLD9qyya7XP9wu4ZfZGir4pof1cmbVaT",
//                    "tag":"member_of"
//                }
//            }
//        },
//        "header":{
//            "entry_type":"LinkAdd",
//            "entry_address":"QmWkS4r25NSqdT7bSSy9X2XRL9ocgTTpL8MVHRKiwxCCR5",
//            "provenances":[["sandwich--------------------------------------------------------------------------AAAEqzh28L","TODO"]],
//            "link":"QmTh7DYYFUcdLAqQKpyz19qQQCeo3HnZnbNEjAzTkCzxPS",
//            "link_same_type":null,
//            "link_crud":null,
//            "timestamp":"1970-01-01T00:00:00+00:00"
//        }
//    });

    pub static ref META_LINK_CONTENT_1: serde_json::Value = json!({"entry":{"content":"hello-meta"}});
    pub static ref META_LINK_CONTENT_2: serde_json::Value = json!({"entry":{"content":"hello-2-meta"}});
    pub static ref META_LINK_CONTENT_3: serde_json::Value = json!({"entry":{"content":"hello-3-meta"}});
}
