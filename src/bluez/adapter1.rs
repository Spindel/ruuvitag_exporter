#![forbid(unsafe_code)]
//! # `DBus` interface proxies for: `org.bluez.Adapter1`, `org.bluez.BatteryProviderManager1`, `org.bluez.GattManager1`, `org.bluez.AdvertisementMonitorManager1`, `org.bluez.Media1`, `org.bluez.NetworkServer1`, `org.bluez.LEAdvertisingManager1`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from `DBus` introspection data.
//! Source: `org.bluez.Adapter1.xml`.
//!
//! You may prefer to adapt it, instead of using it verbatim.
//!
//! More information can be found in the
//! [Writing a client proxy](https://dbus.pages.freedesktop.org/zbus/client.html)
//! section of the zbus documentation.
//!
//! This `DBus` object implements
//! [standard `DBus` interfaces](https://dbus.freedesktop.org/doc/dbus-specification.html),
//! (`org.freedesktop.DBus.*`) for which the following zbus proxies can be used:
//!
//! * [`zbus::fdo::IntrospectableProxy`]
//! * [`zbus::fdo::PropertiesProxy`]
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

#![allow(non_snake_case)]

#[zbus::proxy(interface = "org.bluez.Adapter1", assume_defaults = true)]
trait Adapter1 {
    /// ConnectDevice method
    fn connect_device(
        &self,
        properties: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// GetDiscoveryFilters method
    fn get_discovery_filters(&self) -> zbus::Result<Vec<String>>;

    /// RemoveDevice method
    fn remove_device(&self, device: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// SetDiscoveryFilter method
    fn set_discovery_filter(
        &self,
        properties: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// StartDiscovery method
    fn start_discovery(&self) -> zbus::Result<()>;

    /// StopDiscovery method
    fn stop_discovery(&self) -> zbus::Result<()>;

    /// Address property
    #[zbus(property)]
    fn address(&self) -> zbus::Result<String>;

    /// AddressType property
    #[zbus(property)]
    fn address_type(&self) -> zbus::Result<String>;

    /// Alias property
    #[zbus(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[zbus(property)]
    fn set_alias(&self, value: &str) -> zbus::Result<()>;

    /// Class property
    #[zbus(property)]
    fn class(&self) -> zbus::Result<u32>;

    /// Discoverable property
    #[zbus(property)]
    fn discoverable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_discoverable(&self, value: bool) -> zbus::Result<()>;

    /// DiscoverableTimeout property
    #[zbus(property)]
    fn discoverable_timeout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn set_discoverable_timeout(&self, value: u32) -> zbus::Result<()>;

    /// Discovering property
    #[zbus(property)]
    fn discovering(&self) -> zbus::Result<bool>;

    /// ExperimentalFeatures property
    #[zbus(property)]
    fn experimental_features(&self) -> zbus::Result<Vec<String>>;

    /// Modalias property
    #[zbus(property)]
    fn modalias(&self) -> zbus::Result<String>;

    /// Name property
    #[zbus(property)]
    fn name(&self) -> zbus::Result<String>;

    /// Pairable property
    #[zbus(property)]
    fn pairable(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_pairable(&self, value: bool) -> zbus::Result<()>;

    /// PairableTimeout property
    #[zbus(property)]
    fn pairable_timeout(&self) -> zbus::Result<u32>;
    #[zbus(property)]
    fn set_pairable_timeout(&self, value: u32) -> zbus::Result<()>;

    /// Powered property
    #[zbus(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[zbus(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;

    /// Roles property
    #[zbus(property)]
    fn roles(&self) -> zbus::Result<Vec<String>>;

    /// UUIDs property
    #[zbus(property)]
    fn uuids(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(
    interface = "org.bluez.BatteryProviderManager1",
    assume_defaults = true
)]
trait BatteryProviderManager1 {
    /// RegisterBatteryProvider method
    fn register_battery_provider(
        &self,
        provider: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;

    /// UnregisterBatteryProvider method
    fn unregister_battery_provider(
        &self,
        provider: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.GattManager1", assume_defaults = true)]
trait GattManager1 {
    /// RegisterApplication method
    fn register_application(
        &self,
        application: &zbus::zvariant::ObjectPath<'_>,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// UnregisterApplication method
    fn unregister_application(
        &self,
        application: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;
}

#[zbus::proxy(
    interface = "org.bluez.AdvertisementMonitorManager1",
    assume_defaults = true
)]
trait AdvertisementMonitorManager1 {
    /// RegisterMonitor method
    fn register_monitor(&self, application: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// UnregisterMonitor method
    fn unregister_monitor(&self, application: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// SupportedFeatures property
    #[zbus(property)]
    fn supported_features(&self) -> zbus::Result<Vec<String>>;

    /// SupportedMonitorTypes property
    #[zbus(property)]
    fn supported_monitor_types(&self) -> zbus::Result<Vec<String>>;
}

#[zbus::proxy(interface = "org.bluez.Media1", assume_defaults = true)]
trait Media1 {
    /// RegisterApplication method
    fn register_application(
        &self,
        application: &zbus::zvariant::ObjectPath<'_>,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// RegisterEndpoint method
    fn register_endpoint(
        &self,
        endpoint: &zbus::zvariant::ObjectPath<'_>,
        properties: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// RegisterPlayer method
    fn register_player(
        &self,
        player: &zbus::zvariant::ObjectPath<'_>,
        properties: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// UnregisterApplication method
    fn unregister_application(
        &self,
        application: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;

    /// UnregisterEndpoint method
    fn unregister_endpoint(&self, endpoint: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// UnregisterPlayer method
    fn unregister_player(&self, player: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.NetworkServer1", assume_defaults = true)]
trait NetworkServer1 {
    /// Register method
    fn register(&self, uuid: &str, bridge: &str) -> zbus::Result<()>;

    /// Unregister method
    fn unregister(&self, uuid: &str) -> zbus::Result<()>;
}

#[zbus::proxy(interface = "org.bluez.LEAdvertisingManager1", assume_defaults = true)]
trait LEAdvertisingManager1 {
    /// RegisterAdvertisement method
    fn register_advertisement(
        &self,
        advertisement: &zbus::zvariant::ObjectPath<'_>,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// UnregisterAdvertisement method
    fn unregister_advertisement(
        &self,
        service: &zbus::zvariant::ObjectPath<'_>,
    ) -> zbus::Result<()>;

    /// ActiveInstances property
    #[zbus(property)]
    fn active_instances(&self) -> zbus::Result<u8>;

    /// SupportedCapabilities property
    #[zbus(property)]
    fn supported_capabilities(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;

    /// SupportedFeatures property
    #[zbus(property)]
    fn supported_features(&self) -> zbus::Result<Vec<String>>;

    /// SupportedIncludes property
    #[zbus(property)]
    fn supported_includes(&self) -> zbus::Result<Vec<String>>;

    /// SupportedInstances property
    #[zbus(property)]
    fn supported_instances(&self) -> zbus::Result<u8>;

    /// SupportedSecondaryChannels property
    #[zbus(property)]
    fn supported_secondary_channels(&self) -> zbus::Result<Vec<String>>;
}
