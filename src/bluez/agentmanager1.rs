#![forbid(unsafe_code)]

//! # `DBus` interface proxies for: `org.bluez.AgentManager1`, `org.bluez.ProfileManager1`
//!
//! This code was generated by `zbus-xmlgen` `2.0.1` from `DBus` introspection data.
//! Source: `org.bluez.AgentManager1.xml`.
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
//!
//! …consequently `zbus-xmlgen` did not generate code for the above interfaces.

#[allow(non_snake_case)]
use zbus::dbus_proxy;

#[dbus_proxy(interface = "org.bluez.AgentManager1")]
trait AgentManager1 {
    /// RegisterAgent method
    fn register_agent(
        &self,
        agent: &zbus::zvariant::ObjectPath<'_>,
        capability: &str,
    ) -> zbus::Result<()>;

    /// RequestDefaultAgent method
    fn request_default_agent(&self, agent: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;

    /// UnregisterAgent method
    fn unregister_agent(&self, agent: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}

#[dbus_proxy(interface = "org.bluez.ProfileManager1")]
trait ProfileManager1 {
    /// RegisterProfile method
    fn register_profile(
        &self,
        profile: &zbus::zvariant::ObjectPath<'_>,
        uuid: &str,
        options: std::collections::HashMap<&str, zbus::zvariant::Value<'_>>,
    ) -> zbus::Result<()>;

    /// UnregisterProfile method
    fn unregister_profile(&self, profile: &zbus::zvariant::ObjectPath<'_>) -> zbus::Result<()>;
}
