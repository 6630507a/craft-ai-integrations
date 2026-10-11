//! Consent: what leaves the machine, and whether the user agreed to it.
//!
//! Before a job goes to a remote connection the app shows a [`Disclosure`]: which inputs are
//! sent (by kind and size, never contents) and which parameter names ride along. Agreement is
//! recorded per app and per connection in a [`ConsentLedger`], and is bound to the connection's
//! endpoint fingerprint, so pointing a connection at a different host asks again. Local
//! connections never need consent. The disclosure names exactly what this app sends, which is why
//! consent stays in the app even when the key came from the launcher.

use serde::{Deserialize, Serialize};

use crate::connection::{Connection, Locality};
use crate::endpoint::Endpoint;
use crate::ids::{AppId, ConnectionId};
use crate::model::InputRole;
use crate::payload::PayloadKind;
use crate::request::Request;

/// One thing a request sends.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisclosureItem {
    pub role: InputRole,
    pub kind: PayloadKind,
    pub bytes: Option<u64>,
}

/// What a request will send to a connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Disclosure {
    pub app: AppId,
    pub connection: ConnectionId,
    pub locality: Locality,
    /// `scheme://host[:port]`, or the route name when there is no endpoint.
    pub destination: String,
    pub items: Vec<DisclosureItem>,
    /// Parameter names only, never values.
    pub param_names: Vec<String>,
}

impl Disclosure {
    pub fn for_request(app: &AppId, connection: &Connection, request: &Request) -> Self {
        let destination = connection
            .endpoint
            .as_deref()
            .and_then(|e| Endpoint::parse(e).ok())
            .map(|e| e.fingerprint())
            .unwrap_or_else(|| connection.provider.clone());
        Self {
            app: app.clone(),
            connection: connection.id.clone(),
            locality: connection.locality,
            destination,
            items: request
                .inputs
                .iter()
                .map(|i| DisclosureItem {
                    role: i.role.clone(),
                    kind: i.payload.kind(),
                    bytes: i.payload.size_hint(),
                })
                .collect(),
            param_names: request.params.keys().cloned().collect(),
        }
    }

    /// The distinct kinds of data this request sends.
    pub fn kinds(&self) -> Vec<PayloadKind> {
        let mut kinds: Vec<PayloadKind> = self.items.iter().map(|i| i.kind).collect();
        kinds.sort();
        kinds.dedup();
        kinds
    }
}

/// One saved agreement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentRecord {
    pub app: AppId,
    pub connection: ConnectionId,
    /// The destination agreed to. A different destination needs fresh consent.
    pub destination: String,
    pub kinds: Vec<PayloadKind>,
}

/// An app's saved agreements.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConsentLedger {
    #[serde(default)]
    pub records: Vec<ConsentRecord>,
}

impl ConsentLedger {
    /// True when the user must be asked before this disclosure is sent.
    pub fn needs_consent(&self, d: &Disclosure) -> bool {
        if d.locality == Locality::Local {
            return false;
        }
        let kinds = d.kinds();
        !self.records.iter().any(|r| {
            r.app == d.app
                && r.connection == d.connection
                && r.destination == d.destination
                && kinds.iter().all(|k| r.kinds.contains(k))
        })
    }

    /// Records agreement to the kinds in `d`, merging with any earlier record for the same
    /// destination.
    pub fn grant(&mut self, d: &Disclosure) {
        let kinds = d.kinds();
        if let Some(r) = self.records.iter_mut().find(|r| {
            r.app == d.app && r.connection == d.connection && r.destination == d.destination
        }) {
            for k in kinds {
                if !r.kinds.contains(&k) {
                    r.kinds.push(k);
                }
            }
            r.kinds.sort();
        } else {
            self.records.push(ConsentRecord {
                app: d.app.clone(),
                connection: d.connection.clone(),
                destination: d.destination.clone(),
                kinds,
            });
        }
    }

    pub fn revoke(&mut self, app: &AppId, connection: &ConnectionId) {
        self.records
            .retain(|r| !(&r.app == app && &r.connection == connection));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::capability::CapabilityId;
    use crate::connection::fixtures::{local, remote};
    use crate::payload::Payload;

    fn app() -> AppId {
        AppId::new("photocraft").unwrap()
    }

    fn req() -> Request {
        Request::new(CapabilityId::parse("image.inpaint").unwrap())
            .with_input(
                InputRole::Source,
                Payload::inline("image/png", vec![0; 10]).unwrap(),
            )
            .with_input(InputRole::Prompt, Payload::text("a cat"))
            .with_param("seed", 7.into())
    }

    #[test]
    fn disclosure_lists_kinds_sizes_and_param_names_not_contents() {
        let d = Disclosure::for_request(&app(), &remote("a"), &req());
        assert_eq!(d.destination, "https://api.example.com");
        assert_eq!(d.kinds(), vec![PayloadKind::Text, PayloadKind::Image]);
        assert_eq!(d.items.first().and_then(|i| i.bytes), Some(10));
        assert_eq!(d.param_names, vec!["seed".to_string()]);
        let json = serde_json::to_string(&d).unwrap();
        assert!(!json.contains("a cat"));
    }

    #[test]
    fn local_connections_never_ask() {
        let d = Disclosure::for_request(&app(), &local("a"), &req());
        assert!(!ConsentLedger::default().needs_consent(&d));
    }

    #[test]
    fn remote_asks_until_granted_then_stops() {
        let d = Disclosure::for_request(&app(), &remote("a"), &req());
        let mut ledger = ConsentLedger::default();
        assert!(ledger.needs_consent(&d));
        ledger.grant(&d);
        assert!(!ledger.needs_consent(&d));
    }

    #[test]
    fn a_new_kind_of_data_asks_again() {
        let mut ledger = ConsentLedger::default();
        ledger.grant(&Disclosure::for_request(&app(), &remote("a"), &req()));
        let with_video = req().with_input(
            InputRole::Context,
            Payload::handle("file:///v.mp4", "video/mp4", Some(5)).unwrap(),
        );
        let d = Disclosure::for_request(&app(), &remote("a"), &with_video);
        assert!(ledger.needs_consent(&d));
        ledger.grant(&d);
        assert!(!ledger.needs_consent(&d));
        assert_eq!(ledger.records.len(), 1);
    }

    #[test]
    fn changing_the_host_invalidates_consent() {
        let mut ledger = ConsentLedger::default();
        ledger.grant(&Disclosure::for_request(&app(), &remote("a"), &req()));
        let mut moved = remote("a");
        moved.endpoint = Some("https://other.example.org".into());
        assert!(ledger.needs_consent(&Disclosure::for_request(&app(), &moved, &req())));
    }

    #[test]
    fn consent_is_per_app_and_revocable() {
        let mut ledger = ConsentLedger::default();
        let d = Disclosure::for_request(&app(), &remote("a"), &req());
        ledger.grant(&d);
        let other =
            Disclosure::for_request(&AppId::new("wordcraft").unwrap(), &remote("a"), &req());
        assert!(ledger.needs_consent(&other));
        ledger.revoke(&app(), &remote("a").id);
        assert!(ledger.needs_consent(&d));
    }
}
