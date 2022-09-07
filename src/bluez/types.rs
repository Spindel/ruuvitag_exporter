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
struct DiscoveryFilter {
    #[serde(rename = "UUIDs")]
    Uuids: Vec<String>, // Should be vec of bluetooth UUID
    #[serde(rename = "RSSI")]
    Rssi: i16,          // RSSI threshold value.
    #[serde(rename = "Pathloss")]
    Pathloss: i16,      // RSSI threshold value.
    #[serde(rename = "Transport")]
    Transport: Transport,
    DuplicateData: bool,  // default true
    Discoverable: bool,  // default False
    Pattern: Option<String>,    // "" means match all, applied as OR.
} 
