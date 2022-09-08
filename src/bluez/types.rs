use serde::Deserialize;
use serde::Serialize;
use zbus::zvariant::Type;

#[derive(Deserialize, Serialize, Type, PartialEq, Debug)]
#[zvariant(signature = "s")]
enum Transport {
    #[serde(rename = "auto")]
    Auto,
    #[serde(rename = "bredr")]
    Bredr,
    #[serde(rename = "le")]
    Le,
}

#[derive(Deserialize, Serialize, Type, PartialEq, Debug)]
#[zvariant(signature = "dict")]
pub struct DiscoveryFilter {
    #[serde(rename = "UUIDs")]
    pub uuids: Vec<String>, // Should be vec of bluetooth UUID
    #[serde(rename = "RSSI")]
    pub rssi: i16,          // RSSI threshold value.
    #[serde(rename = "Pathloss")]
    pub pathloss: i16,      // RSSI threshold value.
    #[serde(rename = "Transport")]
    pub transport: Transport,
    #[serde(rename = "DuplicateData")]
    pub duplicatedata: bool,  // default true
    #[serde(rename = "Discoverable")]
    pub discoverable: bool,  // default False
    #[serde(rename = "Pattern")]
    pub pattern: Option<String>,    // "" means match all, applied as OR.
} 
