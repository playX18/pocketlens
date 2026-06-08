use std::{collections::HashMap, net::IpAddr};

use crate::protocol::{PROTOCOL_VERSION, SERVICE_TYPE};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MdnsTxt {
    pub entries: Vec<String>,
}

impl MdnsTxt {
    pub fn build(receiver_name: &str, control_port: u16, capabilities: &[&str]) -> Self {
        Self {
            entries: vec![
                format!("name={receiver_name}"),
                format!("version={PROTOCOL_VERSION}"),
                format!("control_port={control_port}"),
                format!("capabilities={}", capabilities.join(",")),
            ],
        }
    }

    pub fn properties(&self) -> HashMap<String, String> {
        self.entries
            .iter()
            .filter_map(|entry| entry.split_once('='))
            .map(|(key, value)| (key.to_string(), value.to_string()))
            .collect()
    }
}

pub trait MdnsAdvertiser {
    fn advertise(&mut self, service_type: &str, instance_name: &str, port: u16, txt: MdnsTxt);
    fn stop(&mut self);
}

pub fn advertise_receiver<A: MdnsAdvertiser>(
    advertiser: &mut A,
    receiver_name: &str,
    control_port: u16,
) {
    advertiser.advertise(
        SERVICE_TYPE,
        receiver_name,
        control_port,
        MdnsTxt::build(
            receiver_name,
            control_port,
            &["h264", "opus", "rtp", "secure_pairing", "encrypted_rtp"],
        ),
    );
}

pub struct SystemMdnsAdvertiser {
    daemon: mdns_sd::ServiceDaemon,
    fullname: Option<String>,
}

impl SystemMdnsAdvertiser {
    pub fn new() -> Result<Self, mdns_sd::Error> {
        Ok(Self {
            daemon: mdns_sd::ServiceDaemon::new()?,
            fullname: None,
        })
    }
}

impl MdnsAdvertiser for SystemMdnsAdvertiser {
    fn advertise(&mut self, service_type: &str, instance_name: &str, port: u16, txt: MdnsTxt) {
        let host_name = format!(
            "{}.local.",
            instance_name.to_ascii_lowercase().replace(' ', "-")
        );
        let fallback_address = [IpAddr::from([127, 0, 0, 1])];
        match mdns_sd::ServiceInfo::new(
            service_type,
            instance_name,
            &host_name,
            &fallback_address[..],
            port,
            Some(txt.properties()),
        )
        .map(mdns_sd::ServiceInfo::enable_addr_auto)
        {
            Ok(service) => {
                self.fullname = Some(service.get_fullname().to_string());
                if let Err(error) = self.daemon.register(service) {
                    tracing::warn!(%error, "failed to register ACamera mDNS service");
                }
            }
            Err(error) => {
                tracing::warn!(%error, "failed to build ACamera mDNS service info");
            }
        }
    }

    fn stop(&mut self) {
        if let Some(fullname) = self.fullname.take()
            && let Err(error) = self.daemon.unregister(&fullname)
        {
            tracing::warn!(%error, "failed to unregister ACamera mDNS service");
        }
        if let Err(error) = self.daemon.shutdown() {
            tracing::warn!(%error, "failed to shutdown ACamera mDNS daemon");
        }
    }
}

impl Drop for SystemMdnsAdvertiser {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct FakeAdvertiser {
        calls: Vec<(String, String, u16, MdnsTxt)>,
        stopped: bool,
    }

    impl MdnsAdvertiser for FakeAdvertiser {
        fn advertise(&mut self, service_type: &str, instance_name: &str, port: u16, txt: MdnsTxt) {
            self.calls.push((
                service_type.to_string(),
                instance_name.to_string(),
                port,
                txt,
            ));
        }

        fn stop(&mut self) {
            self.stopped = true;
        }
    }

    #[test]
    fn txt_record_contains_plan_fields() {
        let txt = MdnsTxt::build("Desk", 47650, &["h264", "opus"]);
        assert_eq!(
            txt.entries,
            vec![
                "name=Desk",
                "version=1",
                "control_port=47650",
                "capabilities=h264,opus"
            ]
        );
    }

    #[test]
    fn txt_record_properties_match_plan_keys() {
        let txt = MdnsTxt::build("Desk", 47650, &["h264", "opus", "rtp"]);
        let properties = txt.properties();
        assert_eq!(properties.get("name").unwrap(), "Desk");
        assert_eq!(properties.get("version").unwrap(), "1");
        assert_eq!(properties.get("control_port").unwrap(), "47650");
        assert_eq!(properties.get("capabilities").unwrap(), "h264,opus,rtp");
        assert!(!properties.contains_key("proto"));
        assert!(!properties.contains_key("caps"));
    }

    #[test]
    fn advertiser_uses_acamera_service_type() {
        let mut advertiser = FakeAdvertiser::default();
        advertise_receiver(&mut advertiser, "Desk", 47650);
        assert_eq!(advertiser.calls.len(), 1);
        assert_eq!(advertiser.calls[0].0, "_acamera._udp.local");
        assert_eq!(advertiser.calls[0].1, "Desk");
        assert_eq!(advertiser.calls[0].2, 47650);
    }
}
