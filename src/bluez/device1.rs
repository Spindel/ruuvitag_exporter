//! # DBus interface proxy for: `org.bluez.Device1`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from DBus introspection data.
//! Source: `org.bluez.Device1.xml`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://dbus.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This DBus object implements
//! [standard DBus interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

#![allow(non_snake_case)]
use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.bluez.Device1")]
trait Device1 {
    /// CancelPairing method
    fn cancel_pairing(&self) -> zbus::Result<()>;

    /// Connect method
    fn connect(&self) -> zbus::Result<()>;

    /// ConnectProfile method
    fn connect_profile(&self, UUID: &str) -> zbus::Result<()>;

    /// Disconnect method
    fn disconnect(&self) -> zbus::Result<()>;

    /// DisconnectProfile method
    fn disconnect_profile(&self, UUID: &str) -> zbus::Result<()>;

    /// Pair method
    fn pair(&self) -> zbus::Result<()>;

    /// Adapter property
    #[dbus_proxy(property)]
    fn adapter(&self) -> zbus::Result<zbus::zvariant::OwnedObjectPath>;

    /// Address property
    #[dbus_proxy(property)]
    fn address(&self) -> zbus::Result<String>;

    /// AddressType property
    #[dbus_proxy(property)]
    fn address_type(&self) -> zbus::Result<String>;

    /// AdvertisingData property
    #[dbus_proxy(property)]
    fn advertising_data(
        &self,
    ) -> zbus::Result<std::collections::HashMap<u8, zbus::zvariant::OwnedValue>>;

    /// AdvertisingFlags property
    #[dbus_proxy(property)]
    fn advertising_flags(&self) -> zbus::Result<Vec<u8>>;

    /// Alias property
    #[dbus_proxy(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[dbus_proxy(property)]
    fn set_alias(&self, value: &str) -> zbus::Result<()>;

    /// Appearance property
    #[dbus_proxy(property)]
    fn appearance(&self) -> zbus::Result<u16>;

    /// Blocked property
    #[dbus_proxy(property)]
    fn blocked(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_blocked(&self, value: bool) -> zbus::Result<()>;

    /// Bonded property
    #[dbus_proxy(property)]
    fn bonded(&self) -> zbus::Result<bool>;

    /// Class property
    #[dbus_proxy(property)]
    fn class(&self) -> zbus::Result<u32>;

    /// Connected property
    #[dbus_proxy(property)]
    fn connected(&self) -> zbus::Result<bool>;

    /// Icon property
    #[dbus_proxy(property)]
    fn icon(&self) -> zbus::Result<String>;

    /// LegacyPairing property
    #[dbus_proxy(property)]
    fn legacy_pairing(&self) -> zbus::Result<bool>;

    /// ManufacturerData property
    #[dbus_proxy(property)]
    fn manufacturer_data(&self) -> zbus::Result<std::collections::HashMap<u16, std::vec::Vec<u8>>>;

    /// Modalias property
    #[dbus_proxy(property)]
    fn modalias(&self) -> zbus::Result<String>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<String>;

    /// Paired property
    #[dbus_proxy(property)]
    fn paired(&self) -> zbus::Result<bool>;

    /// RSSI property
    #[dbus_proxy(property)]
    fn rssi(&self) -> zbus::Result<i16>;

    /// ServiceData property
    #[dbus_proxy(property)]
    fn service_data(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;

    /// ServicesResolved property
    #[dbus_proxy(property)]
    fn services_resolved(&self) -> zbus::Result<bool>;

    /// Trusted property
    #[dbus_proxy(property)]
    fn trusted(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_trusted(&self, value: bool) -> zbus::Result<()>;

    /// TxPower property
    #[dbus_proxy(property)]
    fn tx_power(&self) -> zbus::Result<i16>;

    /// UUIDs property
    #[dbus_proxy(property)]
    fn uuids(&self) -> zbus::Result<Vec<String>>;

    /// WakeAllowed property
    #[dbus_proxy(property)]
    fn wake_allowed(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_wake_allowed(&self, value: bool) -> zbus::Result<()>;
}