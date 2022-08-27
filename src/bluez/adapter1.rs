//! # DBus interface proxies for: `org.bluez.Adapter1`, `org.bluez.BatteryProviderManager1`, `org.bluez.GattManager1`, `org.bluez.AdvertisementMonitorManager1`, `org.bluez.Media1`, `org.bluez.NetworkServer1`, `org.bluez.LEAdvertisingManager1`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from DBus introspection data.
//! Source: `org.bluez.Adapter1.xml`.
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

use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.bluez.Adapter1")]
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
    #[dbus_proxy(property)]
    fn address(&self) -> zbus::Result<String>;

    /// AddressType property
    #[dbus_proxy(property)]
    fn address_type(&self) -> zbus::Result<String>;

    /// Alias property
    #[dbus_proxy(property)]
    fn alias(&self) -> zbus::Result<String>;
    #[dbus_proxy(property)]
    fn set_alias(&self, value: &str) -> zbus::Result<()>;

    /// Class property
    #[dbus_proxy(property)]
    fn class(&self) -> zbus::Result<u32>;

    /// Discoverable property
    #[dbus_proxy(property)]
    fn discoverable(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_discoverable(&self, value: bool) -> zbus::Result<()>;

    /// DiscoverableTimeout property
    #[dbus_proxy(property)]
    fn discoverable_timeout(&self) -> zbus::Result<u32>;
    #[dbus_proxy(property)]
    fn set_discoverable_timeout(&self, value: u32) -> zbus::Result<()>;

    /// Discovering property
    #[dbus_proxy(property)]
    fn discovering(&self) -> zbus::Result<bool>;

    /// ExperimentalFeatures property
    #[dbus_proxy(property)]
    fn experimental_features(&self) -> zbus::Result<Vec<String>>;

    /// Modalias property
    #[dbus_proxy(property)]
    fn modalias(&self) -> zbus::Result<String>;

    /// Name property
    #[dbus_proxy(property)]
    fn name(&self) -> zbus::Result<String>;

    /// Pairable property
    #[dbus_proxy(property)]
    fn pairable(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_pairable(&self, value: bool) -> zbus::Result<()>;

    /// PairableTimeout property
    #[dbus_proxy(property)]
    fn pairable_timeout(&self) -> zbus::Result<u32>;
    #[dbus_proxy(property)]
    fn set_pairable_timeout(&self, value: u32) -> zbus::Result<()>;

    /// Powered property
    #[dbus_proxy(property)]
    fn powered(&self) -> zbus::Result<bool>;
    #[dbus_proxy(property)]
    fn set_powered(&self, value: bool) -> zbus::Result<()>;

    /// Roles property
    #[dbus_proxy(property)]
    fn roles(&self) -> zbus::Result<Vec<String>>;

    /// UUIDs property
    #[dbus_proxy(property)]
    fn uuids(&self) -> zbus::Result<Vec<String>>;
}

#[dbus_proxy(interface = "org.bluez.BatteryProviderManager1")]
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

#[dbus_proxy(interface = "org.bluez.GattManager1")]
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

#[dbus_proxy(interface = "org.bluez.AdvertisementMonitorManager1")]
trait AdvertisementMonitorManager1 {
    /// RegisterMonitor method
    fn register_monitor(&self, application: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// UnregisterMonitor method
    fn unregister_monitor(&self, application: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// SupportedFeatures property
    #[dbus_proxy(property)]
    fn supported_features(&self) -> zbus::Result<Vec<String>>;

    /// SupportedMonitorTypes property
    #[dbus_proxy(property)]
    fn supported_monitor_types(&self) -> zbus::Result<Vec<String>>;
}

#[dbus_proxy(interface = "org.bluez.Media1")]
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

#[dbus_proxy(interface = "org.bluez.NetworkServer1")]
trait NetworkServer1 {
    /// Register method
    fn register(&self, uuid: &str, bridge: &str) -> zbus::Result<()>;

    /// Unregister method
    fn unregister(&self, uuid: &str) -> zbus::Result<()>;
}

#[dbus_proxy(interface = "org.bluez.LEAdvertisingManager1")]
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
    #[dbus_proxy(property)]
    fn active_instances(&self) -> zbus::Result<u8>;

    /// SupportedCapabilities property
    #[dbus_proxy(property)]
    fn supported_capabilities(
        &self,
    ) -> zbus::Result<std::collections::HashMap<String, zbus::zvariant::OwnedValue>>;

    /// SupportedFeatures property
    #[dbus_proxy(property)]
    fn supported_features(&self) -> zbus::Result<Vec<String>>;

    /// SupportedIncludes property
    #[dbus_proxy(property)]
    fn supported_includes(&self) -> zbus::Result<Vec<String>>;

    /// SupportedInstances property
    #[dbus_proxy(property)]
    fn supported_instances(&self) -> zbus::Result<u8>;

    /// SupportedSecondaryChannels property
    #[dbus_proxy(property)]
    fn supported_secondary_channels(&self) -> zbus::Result<Vec<String>>;
}
