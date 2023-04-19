use std::collections::{HashMap, HashSet};

use crate::commons::channel::SenderEnd;
use crate::commons::crypto::Payload;
use crate::commons::models::state::{Subject, SubjectData};
use crate::identifier::{Derivable, DigestIdentifier, KeyIdentifier};
use crate::message::{MessageConfig, MessageTaskCommand};
use crate::protocol::command_head_manager::self_signature_manager::SelfSignatureInterface;
use crate::Notification;
use crate::{
    database::DB, governance::GovernanceInterface,
    protocol::command_head_manager::self_signature_manager::SelfSignatureManager, DatabaseManager,
    Event,
};

use super::error::{DistributionErrorResponses, DistributionManagerError};
use super::resolutor::DistributionChecksResolutor;
use super::{
    DistributionMessages, RequestEventMessage, RequestSignatureMessage, SetEventMessage,
    SignaturesReceivedMessage,
};
use crate::database::Error as DbError;
use crate::governance::error::RequestError;

const TIMEOUT: u32 = 2000;
const REPLICATION_FACTOR: f64 = 0.5;

pub trait NotifierInterface {
    fn synchrnoization_finished(&self, subject_id: &str);
    fn new_subject(&self, subject_id: &str);
    fn new_event(&self, subject_id: &str, sn: u64);
}

pub struct DistributionNotifier {
    sender: tokio::sync::broadcast::Sender<Notification>,
}

impl DistributionNotifier {
    pub fn new(sender: tokio::sync::broadcast::Sender<Notification>) -> Self {
        Self { sender }
    }
}

impl NotifierInterface for DistributionNotifier {
    fn synchrnoization_finished(&self, subject_id: &str) {
        self.sender
            .send(Notification::subject_synchronized(subject_id));
    }

    fn new_subject(&self, subject_id: &str) {
        self.sender.send(Notification::new_subject(subject_id));
    }

    fn new_event(&self, subject_id: &str, sn: u64) {
        self.sender.send(Notification::new_event(subject_id, sn));
    }
}

pub struct InnerDistributionManager<
    D: DatabaseManager,
    G: GovernanceInterface,
    N: NotifierInterface,
> {
    signature_manager: SelfSignatureManager,
    db: DB<D>,
    governance: G,
    notifier: N,
    messenger_channel: SenderEnd<MessageTaskCommand<DistributionMessages>, ()>,
    synchronization_map: HashMap<DigestIdentifier, (u64, DigestIdentifier, u64)>,
}

/*
 El testigo se comporta de manera similar a los actuales validadores. Rebicirán eventos y deberán comprobar
 su viabilidad. Otorgarán su firma si el evento es correcto y lo propagarán por la red. En caso de haber quorum, no
 realizan el event sourcing hasta que este se alcanza. En caso contrario, sí se realiza.
 En consecuencia distinguimos dos procesos bien diferenciados:
 - Recibir eventos.
 - Propagar eventos.
 El segundo se corresponderá con una tarea autogestionada por el protocolo de mensajería actual.
 Los mensajes que se recibirán serán:
 - SetEvent -> Si el sujeto lo conozco y el evento es el siguiente en la cadena, lo analizo y firmo. Si el sujeto no lo conozco
 pido desde el origen hasta el SN del evento recibido. Si el sujeto lo conozco pero no está al día, he de ponerme al día antes.
 El emisor puede seguir expandiendo su cadena aún sin haber enviado eventos, simulando una cola o una pérdida de conectividad.
 - RequestSignature -> Me han pedido firmas. Si la tengo, se las doy. Si no las tengo, pido el evento.
 - SignaturesReceived -> Me han llegado firmas. Las compruebo y añado al evento si son correctas.

 COMPROBACIONES A REALIZAR CON SET_EVENT
 - Comprobar si somos testigos para el evento/sujeto para el namespace especificado.
 - Comprobar firma del sujeto.
 - Comprobar si el evento necesita de aprobación. En caso afirmativo comprobar las firmas de aprobación.
 - Comprobar firmas de notaría.
 - ¿Comprobar invocación?

 Se plantea el problema de que los eventos pueden ser de versiones anteriores.

*/

impl<D: DatabaseManager, G: GovernanceInterface, N: NotifierInterface>
    InnerDistributionManager<D, G, N>
{
    pub fn new(
        db: DB<D>,
        governance: G,
        signature_manager: SelfSignatureManager,
        notifier: N,
        messenger_channel: SenderEnd<MessageTaskCommand<DistributionMessages>, ()>,
    ) -> Self {
        Self {
            db,
            governance,
            signature_manager,
            notifier,
            messenger_channel,
            synchronization_map: HashMap::new(),
        }
    }

    async fn send_message(
        &self,
        msg: DistributionMessages,
        id: Option<String>,
        targets: Vec<KeyIdentifier>,
    ) -> Result<(), DistributionManagerError> {
        self.messenger_channel
            .tell(MessageTaskCommand::Request(
                id,
                msg,
                targets,
                MessageConfig {
                    timeout: TIMEOUT,
                    replication_factor: REPLICATION_FACTOR,
                },
            ))
            .await
            .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)
    }

    async fn cancel_message(&self, id: String) -> Result<(), DistributionManagerError> {
        self.messenger_channel
            .tell(MessageTaskCommand::Cancel(id))
            .await
            .map_err(|_| DistributionManagerError::MessageChannelNotAvailable)
    }

    pub async fn request_event(
        &self,
        message: RequestEventMessage,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        match self.db.get_event(&message.subject_id, message.sn) {
            Ok(event) => {
                // Intentamoss recuperar las firmas de notaría
                let signatures = match self
                    .db
                    .get_notary_signatures(&message.subject_id, message.sn)
                {
                    Ok((sn, signatures)) => {
                        if sn == message.sn {
                            Some(signatures)
                        } else {
                            None
                        }
                    }
                    Err(DbError::EntryNotFound) => None,
                    Err(error) => {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()))
                    }
                };
                self.send_message(
                    DistributionMessages::SetEvent(SetEventMessage {
                        event,
                        notaries_signatures: signatures,
                        sender: self.signature_manager.get_own_identifier(),
                    }),
                    None,
                    vec![message.sender],
                )
                .await?;
                return Ok(Ok(()));
            }
            Err(DbError::EntryNotFound) => Ok(Err(DistributionErrorResponses::EventNotFound(
                message.sn,
                message.subject_id.to_str(),
            ))),
            Err(error) => Err(DistributionManagerError::DatabaseError(error.to_string())),
        }
    }

    pub async fn request_signatures(
        &self,
        message: RequestSignatureMessage,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        /*
           Si no tenemos el sujeto -> Solicita el evento para el que se piden firmas
           Si no tenemos el evento -> Solicita el evento para el que se piden firmas
           Si tenemos firmas -> Devolvemos las firmas
        */
        // Consultamos si tenemos el sujeto
        match self.db.get_subject(&message.subject_id) {
            Ok(_subject) => {
                // Consultamos si tenemos el evento
                match self.db.get_event(&message.subject_id, message.sn) {
                    Ok(_event) => {
                        // Es posible que ya no tengamos las firmas
                        match self.db.get_signatures(&message.subject_id, message.sn) {
                            Ok(signatures) => {
                                // Filtrado de firmas a las que me piden
                                let signatures = signatures
                                    .into_iter()
                                    .filter(|s| {
                                        message.requested_signatures.contains(&s.content.signer)
                                    })
                                    .collect();
                                self.send_message(
                                    DistributionMessages::SignaturesReceived(
                                        super::SignaturesReceivedMessage {
                                            subject_id: message.subject_id,
                                            sn: message.sn,
                                            signatures: signatures,
                                        },
                                    ),
                                    None,
                                    vec![message.sender],
                                )
                                .await?;
                                return Ok(Ok(()));
                            }
                            Err(DbError::EntryNotFound) => {
                                return Ok(Err(DistributionErrorResponses::SignaturesNotFound));
                            }
                            Err(error) => {
                                return Err(DistributionManagerError::DatabaseError(
                                    error.to_string(),
                                ))
                            }
                        }
                    }
                    Err(DbError::EntryNotFound) => {}
                    Err(error) => {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()))
                    }
                }
            }
            Err(DbError::EntryNotFound) => {}
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        }
        // Es posible que sea un sujeto/evento que nos interese
        // Pedimos el evento que nos piden firmar al remitente
        // Solo se ejecuta en los casos de Subject Not Found o Event Not Found
        self.send_message(
            DistributionMessages::RequestEvent(RequestEventMessage {
                subject_id: message.subject_id,
                sn: message.sn,
                sender: self.signature_manager.get_own_identifier(),
            }),
            None,
            vec![message.sender],
        )
        .await?;
        Ok(Ok(()))
    }

    async fn add_new_event(
        &self,
        event: &Event,
        subject_data: &SubjectData,
    ) -> Result<(), DistributionManagerError> {
        if let Err(error) = self.db.set_event(&subject_data.subject_id, event.clone()) {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        if let Err(error) = self.db.apply_event_sourcing(&event.event_content) {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        Ok(())
    }

    async fn add_new_event_with_signature(
        &self,
        event: &Event,
        subject_data: &SubjectData,
    ) -> Result<(), DistributionManagerError> {
        let Ok(own_signature) = self.signature_manager.sign(&event.event_content) else {
            return Err(DistributionManagerError::SignGenerarionFailed);
        };
        if let Err(error) = self.db.set_signatures(
            &event.event_content.subject_id,
            event.event_content.sn,
            HashSet::from_iter(vec![own_signature]),
        ) {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        self.add_new_event(event, subject_data).await?;
        Ok(())
    }

    async fn add_new_subject(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        let Ok(own_signature) = self.signature_manager.sign(&event.event_content) else {
            return Err(DistributionManagerError::SignGenerarionFailed);
        };
        if let Err(error) = self
            .db
            .set_event(&event.event_content.subject_id, event.clone())
        {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        if let Err(error) = self.db.set_signatures(
            &event.event_content.subject_id,
            event.event_content.sn,
            HashSet::from_iter(vec![own_signature]),
        ) {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        let subject_schema = match self
            .governance
            .get_schema(
                &event.event_content.metadata.governance_id,
                &event.event_content.metadata.schema_id,
            )
            .await
        {
            Ok(result) => result,
            Err(RequestError::SchemaNotFound(id)) => {
                return Ok(Err(DistributionErrorResponses::SchemaNotFound(id)))
            }
            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
        };
        let subject = Subject::new(
            &event.event_content,
            event.signature.content.signer.clone(),
            None,
            &subject_schema,
        )
        .map_err(|_| DistributionManagerError::SubjectCreationError)?;
        if let Err(error) = self
            .db
            .set_subject(&event.event_content.subject_id.clone(), subject)
        {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        Ok(Ok(()))
    }

    async fn add_new_subject_with_signature(
        &self,
        event: &Event,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        let Ok(own_signature) = self.signature_manager.sign(&event.event_content) else {
            return Err(DistributionManagerError::SignGenerarionFailed);
        };
        if let Err(error) = self.db.set_signatures(
            &event.event_content.subject_id,
            event.event_content.sn,
            HashSet::from_iter(vec![own_signature]),
        ) {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        self.add_new_subject(event).await
    }

    pub async fn signature_received(
        &self,
        message: SignaturesReceivedMessage,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // TODO: Debemos comprobar que las firmas sea de los evento que nos indican o, en su defecto que no sean
        // de testigos no válidos.

        // Consultamos si tenemos el sujeto del que nos llegan firmas
        let subject = match self.db.get_subject(&message.subject_id) {
            Ok(subject) => subject,
            Err(DbError::EntryNotFound) => {
                return Ok(Err(DistributionErrorResponses::SubjectNotFound))
            }
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        };
        // Consultamos si las firmas nos son de interés
        if message.sn != subject.subject_data.as_ref().unwrap().sn {
            return Ok(Err(DistributionErrorResponses::SignatureNotNeeded));
        }
        // Comprobamos que las firmas son, en efecto, del evento indicado.
        // A tal fin primero necesitamos recuperar el evento
        let event = match self.db.get_event(&message.subject_id, message.sn) {
            Ok(event) => event,
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        };
        // También comprobaremos si los firmantes son válidos, aunque esto dependerá en gran medida de la versión de la gobernanza
        // TODO: Los "validadores" válidos se tienen que aportar en función de la versión de la gobernanza del evento
        let all_valid_signers = match self.governance.get_validators(event.clone()).await {
            Ok(all_valid_signers) => all_valid_signers,
            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
        };
        for s in message.signatures.iter() {
            if !all_valid_signers.contains(&s.content.signer) {
                return Ok(Err(DistributionErrorResponses::InvalidSigner));
            }
            let data = DigestIdentifier::from_serializable_borsh(&event.event_content)
                .map_err(|_| DistributionManagerError::HashGenerationFailed)?;
            let signature = s.signature.clone();
            if let Err(_error) = s.content.signer.verify(&data.derivative(), signature) {
                return Ok(Err(DistributionErrorResponses::InvalidSignature));
            }
        }
        // Almacenamos las firmas
        if let Err(error) =
            self.db
                .set_signatures(&message.subject_id, message.sn, message.signatures)
        {
            return Err(DistributionManagerError::DatabaseError(error.to_string()));
        }
        // Actualizamos el mensaje de firmas
        // Necesitamos saber que firmas nos hacen falta.
        let current_signers = match self.db.get_signatures(&message.subject_id, message.sn) {
            Ok(current_signatures) => current_signatures
                .into_iter()
                .map(|s| s.content.signer)
                .collect(),
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        };

        let event = match self.db.get_event(&message.subject_id, message.sn) {
            Ok(event) => event,
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        };

        let mut targets = match self.governance.get_validators(event).await {
            Ok(targets) => targets,
            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
        };

        let signatures_remaining: HashSet<KeyIdentifier> =
            targets.difference(&current_signers).cloned().collect();

        if signatures_remaining.is_empty() {
            self.cancel_message(format!("{}/SIGNATURES", message.subject_id.to_str()))
                .await?;
        } else {
            targets.remove(&self.signature_manager.get_own_identifier());
            self.send_message(
                DistributionMessages::RequestSignature(RequestSignatureMessage {
                    subject_id: message.subject_id.clone(),
                    namespace: subject.subject_data.unwrap().namespace,
                    sn: message.sn,
                    sender: self.signature_manager.get_own_identifier(),
                    requested_signatures: signatures_remaining,
                }),
                Some(format!("{}/SIGNATURES", message.subject_id.to_str())),
                Vec::from_iter(targets),
            )
            .await?;
        }
        Ok(Ok(()))
    }

    async fn exec_synchronization(
        &self,
        subject_id: &DigestIdentifier,
    ) -> Result<(), DistributionManagerError> {
        // Rcuperarmos el sujeto de la base de datos para comprobar su estado
        let subject = match self.db.get_subject(subject_id) {
            Ok(subject) => subject,
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        };
        let subject_data = subject.subject_data.unwrap();
        let initial_sn = subject_data.sn + 1;
        // Recuperamos el SN esperado tras la sincronización
        let (sn, _, _) = self.synchronization_map.get(subject_id).unwrap();
        for i in initial_sn..*sn {
            // Recuperamos evento de la base de datos y aplicamos Event Sourcing
            let event = match self.db.get_event(subject_id, i) {
                Ok(event) => event,
                Err(error) => {
                    return Err(DistributionManagerError::DatabaseError(error.to_string()))
                }
            };
            if let Err(error) = self.db.apply_event_sourcing(&event.event_content) {
                return Err(DistributionManagerError::DatabaseError(error.to_string()));
            }
        }
        Ok(())
    }

    async fn ask_for_signatures(&self, event: &Event) -> Result<(), DistributionManagerError> {
        let mut targets = match self.governance.get_validators(event.clone()).await {
            Ok(targets) => targets,
            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
        };
        targets.remove(&self.signature_manager.get_own_identifier());
        self.send_message(
            DistributionMessages::RequestSignature(RequestSignatureMessage {
                subject_id: event.event_content.subject_id.clone(),
                namespace: event.event_content.metadata.namespace.to_owned(),
                sn: event.event_content.sn,
                sender: self.signature_manager.get_own_identifier(),
                requested_signatures: targets.clone(),
            }),
            Some(format!(
                "{}/SIGNATURES",
                event.event_content.subject_id.to_str()
            )),
            Vec::from_iter(targets),
        )
        .await
    }

    pub async fn set_event(
        &mut self,
        message: SetEventMessage,
    ) -> Result<Result<(), DistributionErrorResponses>, DistributionManagerError> {
        // Comprobamos si somos testigo del evento recibido
        if !self
            .governance
            .check_if_witness(
                message.event.event_content.metadata.governance_id.clone(),
                message.event.event_content.metadata.namespace.clone(),
                message.event.event_content.metadata.schema_id.clone(),
            )
            .await
            .unwrap()
        {
            return Ok(Err(DistributionErrorResponses::NoValidWitness));
        }
        let event = &message.event;
        match self.db.get_subject(&message.event.event_content.subject_id) {
            Ok(subject) => {
                let subject_data = subject.subject_data.as_ref().unwrap();
                // Comprobamos si el evento viene con firmas de notaría
                if let Some(signatures) = &message.notaries_signatures {
                    // Tiene firmas de notaría
                    // Podría tratarse del siguiente SN del sujeto o bien de un nuevo candidato para la sincronización
                    if event.event_content.sn == subject_data.sn + 1 {
                        // Se trata del siguiente evento
                        let resolutor = DistributionChecksResolutor::new(
                            &self.synchronization_map,
                            &self.db,
                            &self.governance,
                        );

                        let checks_result = resolutor
                            .with_subject_data(&subject_data)
                            .check_subject_is_signer()
                            .check_event_link()
                            .check_signatures()
                            .check_approvals()
                            .check_notaries_signatures()
                            .check_evaluator_signatures()
                            .execute(&event, &message)
                            .await?;

                        if let Err(error) = checks_result {
                            return Ok(Err(error));
                        }

                        // El evento es correcto. Procedemos a incluirlo en la base de datos y aplicar Event Sourcing.
                        // Asumimos que no hay quorum. Es necesario borrar las firmas del evento anterior.
                        // Es necesario aportar nuestra firma.
                        self.add_new_event_with_signature(event, &subject_data)
                            .await?;
                        // Se debe detener las solicitudes previas de firmas
                        // Procedemos a construir y enviar el mensaje
                        // En la práctica basta con sustituir la tarea previa
                        // TODO: No me gusta que te pida el evento entero.
                        self.ask_for_signatures(event).await?;
                        // Así mismo, guardamos las firmas de notaría del nuevo evento y borramos las del anterior
                        if let Err(error) = self.db.set_notary_signatures(
                            &subject_data.subject_id,
                            event.event_content.sn,
                            signatures.to_owned(),
                        ) {
                            return Err(DistributionManagerError::DatabaseError(error.to_string()));
                        }
                        self.notifier
                            .new_event(&subject_data.subject_id.to_str(), event.event_content.sn);
                        return Ok(Ok(()));
                    } else if event.event_content.sn > subject_data.sn + 1 {
                        // Podría tratarse de un nuevo candidato para la sincronización
                        let resolutor = DistributionChecksResolutor::new(
                            &self.synchronization_map,
                            &self.db,
                            &self.governance,
                        );

                        let checks_result = resolutor
                            .with_subject_data(&subject_data)
                            .check_synchronization_event_needed()
                            .check_notaries_signatures()
                            .check_signatures()
                            .check_approvals()
                            .check_evaluator_signatures()
                            .execute(&event, &message)
                            .await?;

                        let hash = match checks_result {
                            Ok(Some(hash)) => hash,
                            Err(error) => return Ok(Err(error)),
                            _ => unreachable!(),
                        };

                        // El evento parece ser correcto. Procedemos a iniciar un proceso de sincronización
                        // Es posible que ya hubiera una sincronización previa
                        let next_sn = if let Some((_, _, next_sn)) = self
                            .synchronization_map
                            .get(&event.event_content.subject_id)
                        {
                            // Es posible que el sn del evento coincida con next_sn
                            if event.event_content.sn == *next_sn {
                                // En este caso, podríamos finalizar la sincronización
                                // Comprobamos el enlace de este evento con el anterior
                                let prev_event =
                                    match self.db.get_event(&subject_data.subject_id, *next_sn - 1)
                                    {
                                        Ok(prev_event) => prev_event,
                                        Err(error) => {
                                            return Err(DistributionManagerError::DatabaseError(
                                                error.to_string(),
                                            ))
                                        }
                                    };
                                if prev_event.signature.content.event_content_hash
                                    != event.event_content.previous_hash
                                {
                                    // Nuestra cadena aparenta ser incorrecta
                                    // TODO: ESTABLECER MECANISMO DE RECUPERACIÓN
                                }
                                if let Err(error) = self
                                    .db
                                    .set_event(&subject_data.subject_id, event.to_owned())
                                {
                                    return Err(DistributionManagerError::DatabaseError(
                                        error.to_string(),
                                    ));
                                }
                                self.exec_synchronization(&subject_data.subject_id).await?;
                                if let Err(error) = self.db.set_notary_signatures(
                                    &subject_data.subject_id,
                                    event.event_content.sn,
                                    signatures.to_owned(),
                                ) {
                                    return Err(DistributionManagerError::DatabaseError(
                                        error.to_string(),
                                    ));
                                }
                                // Generamos firma
                                let Ok(own_signature) = self.signature_manager.sign(&event.event_content) else {
                                    return Err(DistributionManagerError::SignGenerarionFailed);
                                };
                                if let Err(error) = self.db.set_signatures(
                                    &event.event_content.subject_id,
                                    event.event_content.sn,
                                    HashSet::from_iter(vec![own_signature]),
                                ) {
                                    return Err(DistributionManagerError::DatabaseError(
                                        error.to_string(),
                                    ));
                                }
                                self.cancel_message(format!(
                                    "{}/EVENT",
                                    event.event_content.subject_id.to_str()
                                ))
                                .await?;
                                self.ask_for_signatures(event).await?;
                                self.notifier
                                    .synchrnoization_finished(&subject_data.subject_id.to_str());
                                return Ok(Ok(()));
                            }
                            *next_sn
                        } else {
                            subject_data.sn + 1
                        };

                        self.synchronization_map.insert(
                            event.event_content.subject_id.clone(),
                            (event.event_content.sn, hash, next_sn),
                        );

                        let mut targets = match self.governance.get_validators(event.clone()).await
                        {
                            Ok(targets) => targets,
                            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
                        };
                        targets.remove(&self.signature_manager.get_own_identifier());
                        self.send_message(
                            DistributionMessages::RequestEvent(RequestEventMessage {
                                subject_id: subject_data.subject_id.clone(),
                                sn: subject_data.sn + 1,
                                sender: self.signature_manager.get_own_identifier(),
                            }),
                            Some(format!(
                                // TODO: Quizás debería ser finita
                                "{}/EVENT",
                                event.event_content.subject_id.to_str()
                            )),
                            Vec::from_iter(targets),
                        )
                        .await?;
                    } else {
                        // Ya conocemos el evento
                        return Ok(Err(DistributionErrorResponses::EventNotNeeded));
                    }
                } else {
                    // Carece de firmas de notaría
                    // Tiene que ser el evento siguiente al esperado en la sincronización, si esta existe.
                    let (sn_to_reach, event_hash, next_sn) =
                        match self.synchronization_map.get(&subject_data.subject_id) {
                            None => return Ok(Err(DistributionErrorResponses::EventNotNeeded)),
                            Some(data) => data.clone(),
                        };
                    if next_sn != event.event_content.sn {
                        return Ok(Err(DistributionErrorResponses::EventNotNeeded));
                    }
                    // Es el evento esperado
                    // Debemos comprobar su enganche con el anterior evento
                    let prev_event = match self
                        .db
                        .get_event(&subject_data.subject_id, event.event_content.sn - 1)
                    {
                        Ok(prev_event) => prev_event,
                        Err(error) => {
                            return Err(DistributionManagerError::DatabaseError(error.to_string()))
                        }
                    };

                    if prev_event.signature.content.event_content_hash
                        != event.event_content.previous_hash
                    {
                        // No encaja con el que tenemos guardado
                        return Ok(Err(DistributionErrorResponses::InvalidEventLink));
                    }

                    let resolutor = DistributionChecksResolutor::new(
                        &self.synchronization_map,
                        &self.db,
                        &self.governance,
                    );

                    let checks_result = resolutor
                        .with_subject_data(&subject_data)
                        .check_signatures()
                        .check_approvals()
                        .check_evaluator_signatures()
                        .execute(&event, &message)
                        .await?;

                    if let Err(error) = checks_result {
                        return Ok(Err(error));
                    }

                    // El evento parece correcto.
                    // Es posible que sea el último para acabar la sincronización actual
                    if sn_to_reach == next_sn {
                        // Debemos hacer la comprobación adicional del hash
                        // TODO: Si es el hash del evento o del estado
                        if event_hash != event.signature.content.event_content_hash {
                            // Nos han engañado con los eventos previos. Tenemos una cadena incorrecta
                            // TODO: ESTABLECER MECANISMO DE CORRECCIÓN
                        } else {
                            // Es el hash correcto
                            // Podemos finalizar la sincronización
                            self.synchronization_map.remove(&subject_data.subject_id);
                            if let Err(error) = self
                                .db
                                .set_event(&subject_data.subject_id, event.to_owned())
                            {
                                return Err(DistributionManagerError::DatabaseError(
                                    error.to_string(),
                                ));
                            }
                            self.exec_synchronization(&subject_data.subject_id).await?;
                            if let Err(error) =
                                self.db.delete_notary_signatures(&subject_data.subject_id)
                            {
                                return Err(DistributionManagerError::DatabaseError(
                                    error.to_string(),
                                ));
                            }
                            self.cancel_message(format!(
                                "{}/EVENT",
                                subject_data.subject_id.to_str()
                            ))
                            .await?;
                            // En principio deberíamos de pedir firmas, pero si el evento nos ha llegado sin firmas de notaría entonces
                            // deben haber más eventos en la cadena, por lo que el proceso de sincronización se recuperará en breve.
                            // En consecuencia, no se considera relevante pedir firmas.
                            return Ok(Ok(()));
                        }
                    }
                    if let Err(error) = self
                        .db
                        .set_event(&subject_data.subject_id, event.to_owned())
                    {
                        return Err(DistributionManagerError::DatabaseError(error.to_string()));
                    }
                    // Actualizamos registro de sincronización
                    self.synchronization_map.insert(
                        subject_data.subject_id.clone(),
                        (sn_to_reach, event_hash.to_owned(), next_sn + 1),
                    );
                    // Pedimos el siguiente evento
                    let mut targets = match self.governance.get_validators(event.clone()).await {
                        Ok(targets) => targets,
                        Err(_error) => return Err(DistributionManagerError::UnexpectedError),
                    };
                    // Quitamos al nodo actual de la lista de targets
                    targets.remove(&self.signature_manager.get_own_identifier());
                    self.send_message(
                        DistributionMessages::RequestEvent(RequestEventMessage {
                            subject_id: subject_data.subject_id.to_owned(),
                            sn: next_sn + 1,
                            sender: self.signature_manager.get_own_identifier(),
                        }),
                        Some(format!("{}/EVENT", subject_data.subject_id.to_str())),
                        Vec::from_iter(targets),
                    )
                    .await?;
                    return Ok(Ok(()));
                }
            }
            Err(DbError::EntryNotFound) => {
                // El sujeto no lo conocemos. Puede ser un evento de génesis nuevo o bien nos hemos convertido en testigo
                // El evento de génesis puede no tener firmas de notaría si es debido a una sincronización
                if event.event_content.sn == 0 {
                    // Se trata de un evento de creación
                    if let Some(signatures) = &message.notaries_signatures {
                        // Comprobación integridad criptográfica
                        let resolutor = DistributionChecksResolutor::new(
                            &self.synchronization_map,
                            &self.db,
                            &self.governance,
                        );

                        let checks_result = resolutor
                            .check_signatures()
                            .check_notaries_signatures()
                            .check_evaluator_signatures()
                            .check_approvals()
                            .execute(&event, &message)
                            .await?;

                        if let Err(error) = checks_result {
                            return Ok(Err(error));
                        }

                        if let Err(error) = self.add_new_subject_with_signature(event).await? {
                            return Ok(Err(error));
                        }

                        if let Err(error) = self.db.set_notary_signatures(
                            &event.event_content.subject_id,
                            event.event_content.sn,
                            signatures.to_owned(),
                        ) {
                            return Err(DistributionManagerError::DatabaseError(error.to_string()));
                        }

                        self.ask_for_signatures(event).await?;

                        self.notifier
                            .new_subject(&event.event_content.subject_id.to_str());
                    } else {
                        // No hay firmas de notaría. Solo puede ser válido el evento si existe un intento de sincronización
                        // Comprobar si estamos en un proceso de sincronización
                        let (sn_to_reach, event_hash, next_sn) = match self
                            .synchronization_map
                            .get(&event.event_content.subject_id)
                        {
                            None => return Ok(Err(DistributionErrorResponses::EventNotNeeded)),
                            Some(data) => data.clone(),
                        };
                        if next_sn != 0 {
                            return Ok(Err(DistributionErrorResponses::EventNotNeeded));
                        }
                        // Nos estamos sincronizando. Comprobar utilidad del evento
                        let resolutor = DistributionChecksResolutor::new(
                            &self.synchronization_map,
                            &self.db,
                            &self.governance,
                        );

                        let checks_result = resolutor
                            .check_signatures()
                            .check_evaluator_signatures()
                            .check_approvals()
                            .execute(&event, &message)
                            .await?;

                        if let Err(error) = checks_result {
                            return Ok(Err(error));
                        }
                        // Creamos el sujeto
                        if let Err(error) = self.add_new_subject(event).await? {
                            return Ok(Err(error));
                        }
                        // Solicitamos siguiente mensaje y actualizamos registro de sincronización
                        self.synchronization_map.insert(
                            event.event_content.subject_id.clone(),
                            (sn_to_reach, event_hash, 1),
                        );
                        let mut targets = match self.governance.get_validators(event.clone()).await
                        {
                            Ok(targets) => targets,
                            Err(_error) => return Err(DistributionManagerError::UnexpectedError),
                        };

                        targets.remove(&self.signature_manager.get_own_identifier());

                        self.send_message(
                            DistributionMessages::RequestEvent(RequestEventMessage {
                                subject_id: event.event_content.subject_id.clone(),
                                sn: 1,
                                sender: self.signature_manager.get_own_identifier(),
                            }),
                            Some(format!("{}/EVENT", event.event_content.subject_id.to_str())),
                            Vec::from_iter(targets),
                        )
                        .await?;
                        return Ok(Ok(()));
                    }
                } else {
                    // Nos hemos convertido en testigo y ahora toca sincronizarse
                    // Para ello, es necesario que el evento traiga las firmas de notaría
                    // También podría darse este caso si se nos comunica un evento menor de la cadena
                    // y aún no nos ha dado tiempo de sincronizarnos
                    let resolutor = DistributionChecksResolutor::new(
                        &self.synchronization_map,
                        &self.db,
                        &self.governance,
                    );

                    let checks_result = resolutor
                        .check_notaries_signatures_existence()
                        .check_synchronization_event_needed()
                        .check_notaries_signatures()
                        .check_signatures()
                        .check_approvals()
                        .check_approvals()
                        .execute(&event, &message)
                        .await?;

                    let hash = match checks_result {
                        Ok(Some(hash)) => hash,
                        Err(error) => return Ok(Err(error)),
                        _ => unreachable!(),
                    };

                    // El evento parece ser correcto. Procedemos a iniciar un proceso de sincronización
                    self.synchronization_map.insert(
                        event.event_content.subject_id.clone(),
                        (event.event_content.sn, hash, 0),
                    );

                    // Pedimos el evento 0 al remitente original
                    self.send_message(
                        DistributionMessages::RequestEvent(super::RequestEventMessage {
                            subject_id: event.event_content.subject_id.clone(),
                            sn: 0,
                            sender: self.signature_manager.get_own_identifier(),
                        }),
                        None,
                        vec![message.sender],
                    )
                    .await?;
                }
            }
            Err(error) => return Err(DistributionManagerError::DatabaseError(error.to_string())),
        }
        Ok(Ok(()))
    }
}

// #[cfg(test)]
// mod test {
//     use std::{collections::HashSet, str::FromStr, sync::Arc};

//     use async_trait::async_trait;
//     use serde_json::Value;
//     use tokio::sync::broadcast::Receiver;

//     use crate::{
//         commons::{
//             channel::{ChannelData, MpscChannel},
//             crypto::{Ed25519KeyPair, KeyGenerator, KeyMaterial, KeyPair},
//             models::{notary::NotaryEventResponse, state::Subject},
//         },
//         database::DB,
//         distribution::{
//             error::DistributionErrorResponses, DistributionMessages, RequestEventMessage,
//             RequestSignatureMessage, SetEventMessage, SignaturesReceivedMessage,
//         },
//         evaluator::compiler::ContractType,
//         event_content::Metadata,
//         event_request::{
//             CreateRequest, EventRequest, EventRequestType, RequestPayload, StateRequest,
//         },
//         governance::{error::RequestError, GovernanceInterface, RequestQuorum},
//         identifier::{Derivable, DigestIdentifier, KeyIdentifier},
//         message::MessageTaskCommand,
//         protocol::command_head_manager::self_signature_manager::{
//             SelfSignatureInterface, SelfSignatureManager,
//         },
//         signature::Signature,
//         Event, MemoryManager, Notification, TimeStamp,
//     };

//     use super::{DistributionNotifier, InnerDistributionManager};

//     struct GovernanceMockup {}

//     #[async_trait]
//     impl GovernanceInterface for GovernanceMockup {
//         async fn check_quorum(
//             &self,
//             _event: Event,
//             _signers: &HashSet<KeyIdentifier>,
//         ) -> Result<(bool, HashSet<KeyIdentifier>), RequestError> {
//             unimplemented!();
//         }
//         async fn check_quorum_request(
//             &self,
//             _event_request: EventRequest,
//             _approvals: HashSet<ApprovalResponse>,
//         ) -> Result<(RequestQuorum, HashSet<KeyIdentifier>), RequestError> {
//             unimplemented!();
//         }
//         async fn check_policy(
//             &self,
//             _governance_id: &DigestIdentifier,
//             _governance_version: u64,
//             _schema_id: &String,
//             _subject_namespace: &String,
//             _controller_namespace: &String,
//         ) -> Result<bool, RequestError> {
//             unimplemented!();
//         }
//         async fn get_validators(
//             &self,
//             _event: Event,
//         ) -> Result<HashSet<KeyIdentifier>, RequestError> {
//             return Ok(HashSet::from_iter(vec![
//                 KeyIdentifier::from_str("Eq-ZYOUN8k5LpY_BCvhIcxW8p_5NpTgJB6i2L5EInEeI").unwrap(),
//                 KeyIdentifier::from_str("EL3yBfsyGFUQ64pX6tihpATJR9DqbZWah0Wwekjmx6as").unwrap(),
//                 KeyIdentifier::from_str("EtJKXs4D5a9s_aQYLxtZo0RPSkZsPXak_9SGwmilvc8M").unwrap(),
//             ]));
//         }
//         async fn get_approvers(
//             &self,
//             _event_request: EventRequest,
//         ) -> Result<HashSet<KeyIdentifier>, RequestError> {
//             unimplemented!();
//         }
//         async fn get_governance_version(
//             &self,
//             _governance_id: &DigestIdentifier,
//         ) -> Result<u64, RequestError> {
//             unimplemented!();
//         }
//         async fn get_schema(
//             &self,
//             _governance_id: &DigestIdentifier,
//             _schema_id: &String,
//         ) -> Result<serde_json::Value, RequestError> {
//             Ok(create_subject_schema())
//         }
//         async fn is_governance(
//             &self,
//             _subject_id: &DigestIdentifier,
//         ) -> Result<bool, RequestError> {
//             unimplemented!();
//         }
//         async fn check_invokation_permission(
//             &self,
//             _subject_id: DigestIdentifier,
//             _invokator: KeyIdentifier,
//             _additional_payload: Option<String>,
//             _metadata: Option<Metadata>,
//         ) -> Result<(bool, bool), RequestError> {
//             Ok((true, false))
//         }
//         async fn get_contracts(
//             &self,
//             _governance_id: DigestIdentifier,
//         ) -> Result<Vec<(String, ContractType)>, RequestError> {
//             unimplemented!();
//         }
//         async fn check_if_witness(
//             &self,
//             _governance_id: DigestIdentifier,
//             _namespace: String,
//             _schema_id: String,
//         ) -> Result<bool, RequestError> {
//             Ok(true)
//         }
//         async fn check_notary_signatures(
//             &self,
//             _signatures: HashSet<NotaryEventResponse>,
//             _data_hash: DigestIdentifier,
//             _governance_id: DigestIdentifier,
//             _namespace: String,
//         ) -> Result<(), RequestError> {
//             Ok(())
//         }
//         async fn check_evaluator_signatures(
//             &self,
//             _signatures: HashSet<Signature>,
//             _governance_id: DigestIdentifier,
//             _governance_version: u64,
//             _namespace: String,
//         ) -> Result<(), RequestError> {
//             Ok(())
//         }
//         async fn get_roles_of_invokator(
//             &self,
//             invokator: &KeyIdentifier,
//             governance_id: &DigestIdentifier,
//             governance_version: u64,
//             schema_id: &str,
//             namespace: &str,
//         ) -> Result<Vec<String>, RequestError> {
//             todo!()
//         }
//     }

//     fn create_state_request(
//         json: String,
//         signature_manager: &SelfSignatureManager,
//         subject_id: &DigestIdentifier,
//     ) -> EventRequest {
//         let request = EventRequestType::State(StateRequest {
//             subject_id: subject_id.clone(),
//             payload: RequestPayload::Json(json),
//         });
//         let timestamp = TimeStamp::now();
//         let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
//         let event_request = EventRequest {
//             request,
//             timestamp,
//             signature,
//             approvals: HashSet::new(),
//         };
//         event_request
//     }

//     fn create_state_event(
//         request: EventRequest,
//         subject: &Subject,
//         prev_event_hash: DigestIdentifier,
//         governance_version: u64,
//         subject_schema: &Value,
//     ) -> Event {
//         request
//             .get_event_from_state_request(
//                 subject,
//                 prev_event_hash,
//                 governance_version,
//                 subject_schema,
//                 true,
//             )
//             .unwrap()
//     }

//     fn create_subject(
//         request: EventRequest,
//         governance_version: u64,
//         subject_schema: &Value,
//     ) -> (Subject, Event) {
//         request
//             .create_subject_from_request(governance_version, subject_schema, true)
//             .unwrap()
//     }

//     fn create_genesis_request(
//         json: String,
//         signature_manager: &SelfSignatureManager,
//     ) -> EventRequest {
//         let request = EventRequestType::Create(CreateRequest {
//             governance_id: DigestIdentifier::from_str(
//                 "J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I",
//             )
//             .unwrap(),
//             schema_id: "test".to_owned(),
//             namespace: "test".to_owned(),
//             payload: RequestPayload::Json(json),
//         });
//         let timestamp = TimeStamp::now();
//         let signature = signature_manager.sign(&(&request, &timestamp)).unwrap();
//         let event_request = EventRequest {
//             request,
//             timestamp,
//             signature,
//             approvals: HashSet::new(),
//         };
//         event_request
//     }

//     fn set_event_message(
//         has_signatures: bool,
//         signature_manager: &SelfSignatureManager,
//         event: &Event,
//     ) -> SetEventMessage {
//         SetEventMessage {
//             event: event.clone(),
//             notaries_signatures: if has_signatures {
//                 Some(HashSet::new())
//             } else {
//                 None
//             },
//             sender: signature_manager.get_own_identifier(),
//         }
//     }

//     fn request_event_message(
//         subject_id: &DigestIdentifier,
//         sn: u64,
//         signature_manager: &SelfSignatureManager,
//     ) -> RequestEventMessage {
//         RequestEventMessage {
//             subject_id: subject_id.clone(),
//             sn,
//             sender: signature_manager.get_own_identifier(),
//         }
//     }

//     fn set_signature_message(
//         subject_id: &DigestIdentifier,
//         sn: u64,
//         signatures: HashSet<Signature>,
//     ) -> SignaturesReceivedMessage {
//         SignaturesReceivedMessage {
//             subject_id: subject_id.clone(),
//             sn,
//             signatures,
//         }
//     }

//     fn request_signature_message(
//         subject_id: &DigestIdentifier,
//         sn: u64,
//         signatures_requested: HashSet<KeyIdentifier>,
//         sender: &KeyIdentifier,
//     ) -> RequestSignatureMessage {
//         RequestSignatureMessage {
//             subject_id: subject_id.clone(),
//             namespace: "test".to_owned(),
//             sn,
//             sender: sender.clone(),
//             requested_signatures: signatures_requested,
//         }
//     }

//     fn create_module() -> (
//         InnerDistributionManager<MemoryManager, GovernanceMockup, DistributionNotifier>,
//         MpscChannel<MessageTaskCommand<DistributionMessages>, ()>,
//         Receiver<Notification>,
//         Arc<MemoryManager>,
//         SelfSignatureManager,
//     ) {
//         let database = Arc::new(MemoryManager::new());
//         let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
//             &hex::decode("99beed715bf561185baaa5b3e9df8ecddcfcf7727fbc4f7e922a4cf2f9ea8c4e")
//                 .unwrap(),
//         ));
//         let pk = keypair.public_key_bytes();
//         let signature_manager = SelfSignatureManager {
//             keys: keypair,
//             identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
//             digest_derivator: crate::DigestDerivator::Blake3_256,
//         };
//         let (msg_rx, msg_sx) = MpscChannel::new(100);
//         let governance = GovernanceMockup {};
//         let (notification_sx, notification_rx) = tokio::sync::broadcast::channel(100);
//         let notifier = DistributionNotifier::new(notification_sx);
//         let manager = InnerDistributionManager::new(
//             DB::new(database.clone()),
//             governance,
//             signature_manager.clone(),
//             notifier,
//             msg_sx,
//         );
//         (
//             manager,
//             msg_rx,
//             notification_rx,
//             database,
//             signature_manager,
//         )
//     }

//     fn create_alt_signature_manager() -> SelfSignatureManager {
//         let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
//             &hex::decode("27c775a2f242bd2fea544084f9cc407fb2dbe87f809eb69b80901d4d33da92ef")
//                 .unwrap(),
//         ));
//         let pk = keypair.public_key_bytes();
//         SelfSignatureManager {
//             keys: keypair,
//             identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
//             digest_derivator: crate::DigestDerivator::Blake3_256,
//         }
//     }

//     fn create_third_signature_manager() -> SelfSignatureManager {
//         let keypair = KeyPair::Ed25519(Ed25519KeyPair::from_seed(
//             &hex::decode("7d86b055cf5fe0ebf92b86dcb6122eeb88475e6a9e7eebb8f835cf716b8fda73")
//                 .unwrap(),
//         ));
//         let pk = keypair.public_key_bytes();
//         SelfSignatureManager {
//             keys: keypair,
//             identifier: KeyIdentifier::new(crate::KeyDerivator::Ed25519, &pk),
//             digest_derivator: crate::DigestDerivator::Blake3_256,
//         }
//     }

//     fn create_subject_schema() -> Value {
//         serde_json::json!({"a": {"type": "string"}})
//     }

//     fn create_json_state() -> String {
//         serde_json::to_string(&serde_json::json!({"a": "test"})).unwrap()
//     }

//     fn update_subject_n_times(
//         initial_prev_hash: DigestIdentifier,
//         n: u64,
//         subject: &mut Subject,
//         alt_signature_manager: &SelfSignatureManager,
//     ) -> Event {
//         let mut prev_hash = initial_prev_hash;
//         for i in 0..n {
//             let event = create_state_event(
//                 create_state_request(
//                     create_json_state(),
//                     &alt_signature_manager,
//                     &subject.subject_data.as_ref().unwrap().subject_id,
//                 ),
//                 &subject,
//                 prev_hash.clone(),
//                 0,
//                 &create_subject_schema(),
//             );
//             prev_hash = event.signature.content.event_content_hash.clone();
//             subject.apply(&event.event_content).unwrap();
//             if i == n - 1 {
//                 return event;
//             }
//         }
//         unreachable!();
//     }

//     fn check_subject_and_event(
//         database: &DB<MemoryManager>,
//         subject_id: &DigestIdentifier,
//         sn: u64,
//     ) {
//         let result = database.get_subject(subject_id);
//         assert!(result.is_ok());
//         let result = database.get_event(subject_id, 0);
//         assert!(result.is_ok());
//     }

//     fn check_cancel_message(
//         msg: ChannelData<MessageTaskCommand<DistributionMessages>, ()>,
//         id: String,
//     ) {
//         let ChannelData::TellData(data) = msg else {
//             assert!(false);
//             return;
//         };
//         let data = data.get();
//         let MessageTaskCommand::Cancel(id_to_cancel) = data else {
//             assert!(false);
//             return;
//         };
//         assert_eq!(id, id_to_cancel);
//     }

//     fn check_request_signatures(
//         msg: ChannelData<MessageTaskCommand<DistributionMessages>, ()>,
//         subject_data: &SubjectData,
//         signature_manager_inner: &SelfSignatureManager,
//         sn: u64,
//     ) {
//         let ChannelData::TellData(data) = msg else {
//             assert!(false);
//             return;
//         };

//         let data = data.get();
//         let MessageTaskCommand::Request(id, data, targets, _) = data else {
//             assert!(false);
//             return;
//         };

//         assert!(id.is_some());
//         let id = id.unwrap();
//         assert_eq!(
//             format!("{}/SIGNATURES", subject_data.subject_id.to_str()),
//             id
//         );
//         assert_eq!(targets.len(), 2);
//         let DistributionMessages::RequestSignature(data) = data else {
//             assert!(false);
//             return;
//         };
//         assert!(!targets.contains(&signature_manager_inner.get_own_identifier()));
//         assert_eq!(data.subject_id, subject_data.subject_id);
//         assert_eq!(data.sn, sn);
//         assert_eq!(data.sender, signature_manager_inner.get_own_identifier());
//     }

//     fn check_requested_signature(
//         msg: ChannelData<MessageTaskCommand<DistributionMessages>, ()>,
//         sn: u64,
//         subject_id: &DigestIdentifier,
//         target: &KeyIdentifier,
//         signatures_expected: usize,
//     ) {
//         let ChannelData::TellData(data) = msg else {
//             assert!(false);
//             return;
//         };
//         let data = data.get();
//         let MessageTaskCommand::Request(id, data, targets, _) = data else {
//             assert!(false);
//             return;
//         };
//         assert!(id.is_none());
//         assert_eq!(targets.len(), 1);
//         assert_eq!(targets[0], *target);
//         let DistributionMessages::SignaturesReceived(data) = data else {
//             assert!(false);
//             return;
//         };
//         assert_eq!(data.sn, sn);
//         assert_eq!(data.subject_id, *subject_id);
//         assert_eq!(signatures_expected, data.signatures.len());
//     }

//     fn check_provide_event(
//         msg: ChannelData<MessageTaskCommand<DistributionMessages>, ()>,
//         sn: u64,
//         subject_id: &DigestIdentifier,
//         target: &KeyIdentifier,
//     ) {
//         let ChannelData::TellData(data) = msg else {
//             assert!(false);
//             return;
//         };
//         let data = data.get();
//         let MessageTaskCommand::Request(id, data, targets, _) = data else {
//             assert!(false);
//             return;
//         };
//         assert!(id.is_none());
//         assert_eq!(targets.len(), 1);
//         assert_eq!(targets[0], *target);
//         let DistributionMessages::SetEvent(data) = data else {
//             assert!(false);
//             return;
//         };
//         assert_eq!(data.event.event_content.sn, sn);
//         assert_eq!(data.event.event_content.subject_id, *subject_id);
//     }

//     fn check_request_event(
//         msg: ChannelData<MessageTaskCommand<DistributionMessages>, ()>,
//         subject_data: &SubjectData,
//         signature_manager_inner: &SelfSignatureManager,
//         sn: u64,
//         targets_len: usize,
//         have_id: bool,
//     ) {
//         let ChannelData::TellData(data) = msg else {
//             assert!(false);
//             return;
//         };

//         let data = data.get();
//         let MessageTaskCommand::Request(id, data, targets, _) = data else {
//             assert!(false);
//             return;
//         };

//         if have_id {
//             assert!(id.is_some());
//             let id = id.unwrap();
//             assert_eq!(format!("{}/EVENT", subject_data.subject_id.to_str()), id);
//         }
//         assert_eq!(targets.len(), targets_len);
//         let DistributionMessages::RequestEvent(data) = data else {
//             assert!(false);
//             return;
//         };
//         assert!(!targets.contains(&signature_manager_inner.get_own_identifier()));
//         assert_eq!(data.subject_id, subject_data.subject_id);
//         assert_eq!(data.sn, sn);
//         assert_eq!(data.sender, signature_manager_inner.get_own_identifier());
//     }

//     #[test]
//     fn genesis_event() {
//         /*
//            La idea de este Test es simular la llegada de un evento de génesis de un sujeto que nos es de interés
//         */
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_data = subject.subject_data.unwrap();

//             let result = manager
//                 .set_event(set_event_message(
//                     true,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el nuevo sujeto y su evento 0
//             check_subject_and_event(&database, &subject_data.subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             // El mensaje es de tipo TELL
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 0);
//         });
//     }

//     #[test]
//     fn new_event() {
//         // Existe la posibilidad de modificar la base de datos antes de ejecutar el método. De esta manera se
//         // podría alterar la ejecución interna del módulo. No obstante, vamos a optar por recrear el proceso desde cero,
//         // creación del sujeto incluída.
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let prev_hash = genesis_event.signature.content.event_content_hash.clone();

//             let subject_data = subject.subject_data.as_ref().unwrap();

//             let result = manager
//                 .set_event(set_event_message(
//                     true,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el nuevo sujeto y su evento 0
//             check_subject_and_event(&database, &subject_data.subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 0);
//             // Enviamos un nuevo mensaje con el nuevo evento
//             let event = create_state_event(
//                 create_state_request(
//                     create_json_state(),
//                     &alt_signature_manager,
//                     &subject.subject_data.as_ref().unwrap().subject_id,
//                 ),
//                 &subject,
//                 prev_hash,
//                 0,
//                 &create_subject_schema(),
//             );
//             let result = manager
//                 .set_event(set_event_message(true, &alt_signature_manager, &event))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el sujeto y su evento 1
//             check_subject_and_event(&database, &subject_data.subject_id, 1);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, subject_data, &signature_manager_inner, 1);
//         });
//     }

//     #[test]
//     fn new_witness() {
//         // Se creará un sujeto y una serie de eventos. El último de estos se pasará al módulo.
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (mut subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let event = update_subject_n_times(
//                 genesis_event.signature.content.event_content_hash,
//                 3,
//                 &mut subject,
//                 &alt_signature_manager,
//             );

//             let subject_data = subject.subject_data.unwrap();
//             let result = manager
//                 .set_event(set_event_message(true, &alt_signature_manager, &event))
//                 .await;
//             assert!(result.is_ok());
//             // En la BBDD no debería haber sujeto alguno. Se habrá iniciado un proceso de sincronización
//             // Tampoco debería haber sujeto
//             let result = database.get_subject(&subject_data.subject_id);
//             assert!(result.is_err());
//             let result = database.get_event(&subject_data.subject_id, 3);
//             assert!(result.is_err());
//             // Se debería haber generado una petición para obtención del evento 0
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 0, 1, false);
//         });
//     }

//     #[test]
//     fn new_witness_provide_genesis() {
//         // Se creará un sujeto y una serie de eventos. El último de estos se pasará al módulo.
//         // Acto seguido se pasará el evento de génesis SIN firmas de notario.
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (mut subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let genesis_copy = genesis_event.clone();

//             let event = update_subject_n_times(
//                 genesis_event.signature.content.event_content_hash,
//                 3,
//                 &mut subject,
//                 &alt_signature_manager,
//             );

//             let subject_data = subject.subject_data.unwrap();
//             let result = manager
//                 .set_event(set_event_message(true, &alt_signature_manager, &event))
//                 .await;
//             assert!(result.is_ok());
//             // En la BBDD no debería haber sujeto alguno. Se habrá iniciado un proceso de sincronización
//             // Tampoco debería haber sujeto
//             let result = database.get_subject(&subject_data.subject_id);
//             assert!(result.is_err());
//             let result = database.get_event(&subject_data.subject_id, 3);
//             assert!(result.is_err());
//             // Se debería haber generado una petición para obtención del evento 0
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 0, 1, false);

//             // Pasamos el evento de génesis sin formas de notario
//             let result = manager
//                 .set_event(set_event_message(
//                     false,
//                     &alt_signature_manager,
//                     &genesis_copy,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos debería estar ahora el sujeto + el evento de génesis
//             check_subject_and_event(&database, &subject_data.subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud del siguiente evento
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 1, 2, true);
//         });
//     }

//     #[test]
//     fn provide_signatures() {
//         // Se creará un sujeto y se proveerán las firmas necesarias
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_data = subject.subject_data.unwrap();

//             let result = manager
//                 .set_event(set_event_message(
//                     true,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el nuevo sujeto y su evento 0
//             check_subject_and_event(&database, &subject_data.subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 0);

//             // El evento ha sido creado con éxito.
//             // Generamos la tercera entidad firmante necesaria
//             let third_signer = create_third_signature_manager();
//             // Generamos las 2 firmas necesarias. Las proveeremos de una en una.
//             let alt_signature = alt_signature_manager
//                 .sign(&genesis_event.event_content)
//                 .unwrap();
//             let third_signature = third_signer.sign(&genesis_event.event_content).unwrap();
//             let result = manager
//                 .signature_received(set_signature_message(
//                     &subject_data.subject_id,
//                     subject_data.sn,
//                     HashSet::from_iter(vec![alt_signature]),
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 0);
//             let result = manager
//                 .signature_received(set_signature_message(
//                     &subject_data.subject_id,
//                     subject_data.sn,
//                     HashSet::from_iter(vec![third_signature]),
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En msg_rx deberíamos tener un mensaje para CANCELAR la solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_cancel_message(
//                 msg,
//                 format!("{}/SIGNATURES", subject_data.subject_id.to_str()),
//             );
//             // En la base de datos deberíamos tener 3 firmas
//             let result = database.get_signatures(&subject_data.subject_id, 0);
//             assert!(result.is_ok());
//             let result = result.unwrap();
//             assert_eq!(result.len(), 3);
//         });
//     }

//     #[test]
//     fn invalid_genesis() {
//         // Se creará un sujeto y se pasará su evento de génesis, pero sin firmas de notaría
//         // El módulo debería rechazarlo.
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, _msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_data = subject.subject_data.unwrap();
//             let result = manager
//                 .set_event(set_event_message(
//                     false,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             let result = result.unwrap();
//             let Err(DistributionErrorResponses::EventNotNeeded) = result else {
//                 assert!(false);
//                 return;
//             };
//             // En la base de datos no debería haber ni sujeto ni evento
//             let result = database.get_subject(&subject_data.subject_id);
//             assert!(result.is_err());
//             let result = database.get_event(&subject_data.subject_id, 0);
//             assert!(result.is_err());
//         });
//     }

//     #[test]
//     fn request_event() {
//         // Partiendo de una base de datos con un sujeto y eventos, se pedirá al módulo que entregue un evento.
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_id = subject.subject_data.as_ref().unwrap().subject_id.clone();

//             // Almacenamos en BBDD
//             let result = database.set_subject(&subject_id, subject);
//             assert!(result.is_ok());
//             let result = database.set_event(&subject_id, genesis_event);
//             assert!(result.is_ok());

//             let result = manager
//                 .request_event(request_event_message(
//                     &subject_id,
//                     0,
//                     &alt_signature_manager,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En msg_rx deberíamos tener un mensaje para entregar el evento
//             let msg = msg_rx.receive().await.unwrap();
//             check_provide_event(
//                 msg,
//                 0,
//                 &subject_id,
//                 &alt_signature_manager.get_own_identifier(),
//             );
//         });
//     }

//     #[test]
//     fn request_signature() {
//         // Partiendo de una base de datos con contenido, se solicitará al módulo que entregue firmas
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_id = subject.subject_data.as_ref().unwrap().subject_id.clone();

//             let node_signature = signature_manager_inner
//                 .sign(&genesis_event.event_content)
//                 .unwrap();

//             // Almacenamos en BBDD
//             let result = database.set_subject(&subject_id, subject);
//             assert!(result.is_ok());
//             let result = database.set_event(&subject_id, genesis_event);
//             assert!(result.is_ok());
//             let result =
//                 database.set_signatures(&subject_id, 0, HashSet::from_iter(vec![node_signature]));
//             assert!(result.is_ok());

//             let result = manager
//                 .request_signatures(request_signature_message(
//                     &subject_id,
//                     0,
//                     HashSet::from_iter(vec![signature_manager_inner.get_own_identifier()]),
//                     &alt_signature_manager.get_own_identifier(),
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En msg_rx deberíamos tener un mensaje para entregar la firma
//             let msg = msg_rx.receive().await.unwrap();
//             check_requested_signature(
//                 msg,
//                 0,
//                 &subject_id,
//                 &alt_signature_manager.get_own_identifier(),
//                 1,
//             );
//         });
//     }

//     #[test]
//     fn synchronization_test() {
//         // Se creará un evento y un sujeto y se comunicará al módulo. Acto seguido se crearán más eventos y se pasará el último de estos
//         // para simular la sincronización
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (mut subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_id = subject.subject_data.as_ref().unwrap().subject_id.clone();

//             let result = manager
//                 .set_event(set_event_message(
//                     true,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el nuevo sujeto y su evento 0
//             check_subject_and_event(&database, &subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             // El mensaje es de tipo TELL
//             check_request_signatures(
//                 msg,
//                 &subject.subject_data.as_ref().unwrap(),
//                 &signature_manager_inner,
//                 0,
//             );

//             // A continuación creamos el 3 eventos, uno por uno.
//             let event_1 = update_subject_n_times(
//                 genesis_event.signature.content.event_content_hash,
//                 1,
//                 &mut subject,
//                 &alt_signature_manager,
//             );

//             let event_2 = update_subject_n_times(
//                 event_1.signature.content.event_content_hash.clone(),
//                 1,
//                 &mut subject,
//                 &alt_signature_manager,
//             );

//             let event_3 = update_subject_n_times(
//                 event_2.signature.content.event_content_hash.clone(),
//                 1,
//                 &mut subject,
//                 &alt_signature_manager,
//             );

//             let subject_data = subject.subject_data.as_ref().unwrap().clone();

//             // Pasamos el evento 3. Debe tener firmas de notaría
//             let result = manager
//                 .set_event(set_event_message(true, &alt_signature_manager, &event_3))
//                 .await;
//             assert!(result.is_ok());
//             // El módulo ha registrado el evento para la sincronización, pero no lo ha guardado en BBDD
//             // En msg_rx deberíamos tener un mensaje de solicitud del evento 1
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 1, 2, true);
//             // Proveeremos el resto de eventos
//             let result = manager
//                 .set_event(set_event_message(false, &alt_signature_manager, &event_1))
//                 .await;
//             assert!(result.is_ok());
//             // En msg_rx deberíamos tener un mensaje de solicitud del evento 2
//             // En la BBDD se deberían encontrar los eventos 1 y 2 aunque el sujeto no debe cambiar su estado hasta que se complete
//             // la sincronización
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 2, 2, true);
//             let result = manager
//                 .set_event(set_event_message(false, &alt_signature_manager, &event_2))
//                 .await;
//             assert!(result.is_ok());
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_event(msg, &subject_data, &signature_manager_inner, 3, 2, true);
//             // El evento 1 y 2 deberían estar en la BBDD
//             let result = database.get_event(&subject_id, 0);
//             assert!(result.is_ok());
//             let result = database.get_event(&subject_id, 0);
//             assert!(result.is_ok());
//             // El sujeto no debería haber cambiado
//             let result = database.get_subject(&subject_id);
//             assert!(result.is_ok());
//             let result = result.unwrap();
//             assert_eq!(result.subject_data.unwrap().sn, 0);
//             // Proveemos el último evento, acabando la sincronización
//             let result = manager
//                 .set_event(set_event_message(true, &alt_signature_manager, &event_3))
//                 .await;
//             assert!(result.is_ok());
//             // Debería de generarse un mensaje para la cancelación de los eventos
//             let msg = msg_rx.receive().await.unwrap();
//             check_cancel_message(msg, format!("{}/EVENT", subject_id.to_str()));
//             // Debería de generarse un mensaje para la obtención de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 3);
//         });
//     }

//     #[test]
//     fn activate_event_request_after_signature_request() {
//         /*
//            La intención de este Test es comprobar que el módulo pide un desconocido ante una petición de
//            firmas que no puede sastisfacer
//         */
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let alt_signature_manager = create_alt_signature_manager();
//             let subject_id =
//                 DigestIdentifier::from_str("J6axKnS5KQjtMDFgapJq49tdIpqGVpV7SS4kxV1iR10I").unwrap();
//             let result = manager
//                 .request_signatures(request_signature_message(
//                     &subject_id,
//                     4,
//                     HashSet::from_iter(vec![signature_manager_inner.get_own_identifier()]),
//                     &alt_signature_manager.get_own_identifier(),
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // Habrá una solicitud del evento anterior
//             let msg = msg_rx.receive().await.unwrap();
//             let ChannelData::TellData(data) = msg else {
//                 assert!(false);
//                 return;
//             };

//             let data = data.get();
//             let MessageTaskCommand::Request(id, data, targets, _) = data else {
//                 assert!(false);
//                 return;
//             };
//             assert!(id.is_none());
//             assert_eq!(targets.len(), 1);
//             let DistributionMessages::RequestEvent(data) = data else {
//                 assert!(false);
//                 return;
//             };
//             assert!(!targets.contains(&signature_manager_inner.get_own_identifier()));
//             assert_eq!(data.subject_id, subject_id);
//             assert_eq!(data.sn, 4);
//             assert_eq!(data.sender, signature_manager_inner.get_own_identifier());
//         });
//     }

//     #[test]
//     fn reject_invalid_signature() {
//         let rt = tokio::runtime::Runtime::new().unwrap();
//         rt.block_on(async {
//             let (mut manager, mut msg_rx, _notif_rx, db, signature_manager_inner) = create_module();
//             let database = DB::new(db);
//             let alt_signature_manager = create_alt_signature_manager();

//             let (subject, genesis_event) = create_subject(
//                 create_genesis_request(create_json_state(), &alt_signature_manager),
//                 0,
//                 &create_subject_schema(),
//             );

//             let subject_data = subject.subject_data.unwrap();

//             let result = manager
//                 .set_event(set_event_message(
//                     true,
//                     &alt_signature_manager,
//                     &genesis_event,
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             // En la base de datos deberíamos tener el nuevo sujeto y su evento 0
//             check_subject_and_event(&database, &subject_data.subject_id, 0);
//             // En msg_rx deberíamos tener un mensaje de solicitud de firmas
//             let msg = msg_rx.receive().await.unwrap();
//             check_request_signatures(msg, &subject_data, &signature_manager_inner, 0);

//             // El evento ha sido creado con éxito.
//             // Ahora vamos a enviar una firma inválida
//             let signature = alt_signature_manager.sign(&43).unwrap();
//             let result = manager
//                 .signature_received(set_signature_message(
//                     &subject_data.subject_id,
//                     subject_data.sn,
//                     HashSet::from_iter(vec![signature]),
//                 ))
//                 .await;
//             assert!(result.is_ok());
//             let result = result.unwrap();
//             assert!(result.is_err());
//         });
//     }
// }
