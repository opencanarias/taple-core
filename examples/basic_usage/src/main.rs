use core::{ApiModuleInterface, Taple, identifier::Derivable};
use std::{error::Error, time::Duration};
use commons::crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let mut settings = Taple::get_default_settings();
    // Generate ramdon cryptographic material
    let keypair = Ed25519KeyPair::from_seed(&[]);
    let hex_private_key = hex::encode(&keypair.secret_key_bytes());
    settings.node.secret_key = Some(hex_private_key);
    
    let mut taple = Taple::new(settings);
    // The TAPLE node generates several Tokyo tasks to manage the different
    // components of its architecture.
    // The "start" method initiates these tasks and returns the control flow.
    taple.start().await.expect("TAPLE started");
    // From this point the user can start interacting with the node.
    // It is the user's responsibility to decide whether to keep the node running.
    // To do so, the main thread of the application must not terminate.
    let api = taple.get_api();

    // First we need to create the governance, the game set of rules of our future network, to start creating subject on it.
    let payload = taple.get_default_governance();

    // Next we will send the request to create a governance and we will save the response in a variable for later use.
    let response = api
        .create_governance(payload)
        .await
        .expect("Error getting server response");
    let subject_id = response
        .subject_id
        .expect("Error.Response returned empty subject_id");

    // wait until validation phase is resolved
    let max_attemps = 4;
    let mut attemp = 0;
    while attemp <= max_attemps {
        if let Ok(data) = api.get_signatures(subject_id.clone(), 0, None, None).await {
            if data.len() == 1 {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        attemp += 1;
    }
    // Our governance is treated like a subject so, when we create it, inside the response, we have it's subject_id.
    // We can use this to retrieve our governance data:
    let subject = api.get_subject(subject_id.clone()).await.expect(&format!(
        "Error getting subject content with id: {}",
        subject_id
    ));

    println!("Governance subject Id: {:#?}", subject.subject_id.to_str());
    println!("Governance subject SN: {:#?}", subject.sn);

    // Now we send a signal to stop our TAPLE node:
    api.shutdown().await.expect("TAPLE shutdown");
    Ok(())
}
