use libp2p::identity::ed25519::Keypair as EdKeyPair;
use libp2p::PeerId;
use taple_core::{
    crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair},
    request::StartRequest,
    signature::{Signature, Signed},
    Api, DigestIdentifier, EventRequest, KeyIdentifier, SubjectData,
};

pub async fn check_subject(
    node_api: &Api,
    subject_id: &DigestIdentifier,
    sn: Option<u64>,
) -> SubjectData {
    let result = node_api.get_subject(subject_id.clone()).await;
    assert!(result.is_ok());
    let subject = result.unwrap();
    assert_eq!(subject.subject_id, *subject_id);
    if let Some(sn) = sn {
        assert_eq!(subject.sn, sn);
    }
    subject
}

pub struct McNodeData {
    keys: KeyPair,
    peer_id: PeerId,
}

impl McNodeData {
    pub fn get_private_key(&self) -> String {
        let private_key = self.keys.secret_key_bytes();
        hex::encode(private_key)
    }

    pub fn get_controller_id(&self) -> KeyIdentifier {
        KeyIdentifier::new(self.keys.get_key_derivator(), &self.keys.public_key_bytes())
    }

    #[allow(dead_code)]
    pub fn get_peer_id(&self) -> PeerId {
        self.peer_id.clone()
    }

    pub fn sign_event_request(&self, content: &EventRequest) -> Signed<EventRequest> {
        Signed {
            content: content.clone(),
            signature: Signature::new(content, &self.keys).unwrap(),
        }
    }
}

pub fn generate_mc() -> McNodeData {
    let keys = Ed25519KeyPair::from_seed(&[]);
    let peer_id = PeerId::from_public_key(
        &libp2p::identity::Keypair::Ed25519(
            EdKeyPair::decode(&mut keys.to_bytes()).expect("Decode of Ed25519 possible"),
        )
        .public(),
    );
    let keys = KeyPair::Ed25519(keys);
    McNodeData { keys, peer_id }
}

pub fn create_governance_request<S: Into<String>>(
    namespace: S,
    public_key: KeyIdentifier,
    name: S,
) -> EventRequest {
    EventRequest::Create(StartRequest {
        governance_id: DigestIdentifier::default(),
        schema_id: "governance".into(),
        namespace: namespace.into(),
        name: name.into(),
        public_key,
    })
}
