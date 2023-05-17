use std::collections::HashSet;

use crate::{
    commons::channel::SenderEnd,
    database::DB,
    distribution::LedgerMessages,
    ledger::LedgerCommand,
    message::{MessageConfig, MessageTaskCommand},
    protocol::protocol_message_manager::TapleMessages,
    DatabaseManager, DigestIdentifier, KeyIdentifier,
};

use super::error::AuthorizedSubjectsError;

/// Estructura que maneja los sujetos preautorizados en un sistema y se comunica con otros componentes del sistema a través de un canal de mensajes.
pub struct AuthorizedSubjects<D: DatabaseManager> {
    /// Objeto que maneja la conexión a la base de datos.
    database: DB<D>,
    /// Canal de mensajes que se utiliza para comunicarse con otros componentes del sistema.
    message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
    /// Identificador único para el componente que utiliza esta estructura.
    our_id: KeyIdentifier,
}

impl<D: DatabaseManager> AuthorizedSubjects<D> {
    /// Crea una nueva instancia de la estructura `AuthorizedSubjects`.
    ///
    /// # Arguments
    ///
    /// * `database` - Conexión a la base de datos.
    /// * `message_channel` - Canal de mensajes.
    /// * `our_id` - Identificador único.
    pub fn new(
        database: DB<D>,
        message_channel: SenderEnd<MessageTaskCommand<TapleMessages>, ()>,
        our_id: KeyIdentifier,
    ) -> Self {
        Self {
            database,
            message_channel,
            our_id,
        }
    }

    /// Obtiene todos los sujetos preautorizados y envía un mensaje a los proveedores asociados a través del canal de mensajes.
    ///
    /// # Errors
    ///
    /// Devuelve un error si no se pueden obtener los sujetos preautorizados o si no se puede enviar un mensaje a través del canal de mensajes.
    pub async fn ask_for_all(&self) -> Result<(), AuthorizedSubjectsError> {
        // Obtenemos todos los sujetos preautorizados de la base de datos.
        let preauthorized_subjects = self
            .database
            .get_preauthorized_subjects_and_providers(None, 10000)?;

        // Para cada sujeto preautorizado, enviamos un mensaje a los proveedores asociados a través del canal de mensajes.
        for (subject_id, providers) in preauthorized_subjects.into_iter() {
            if !providers.is_empty() {
                self.message_channel
                    .tell(MessageTaskCommand::Request(
                        None,
                        TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
                            who_asked: self.our_id.clone(),
                            subject_id,
                        }),
                        providers.into_iter().collect(),
                        MessageConfig::direct_response(),
                    ))
                    .await?;
            }
        }
        Ok(())
    }

    /// Agrega un nuevo sujeto preautorizado y envía un mensaje a los proveedores asociados a través del canal de mensajes.
    ///
    /// # Arguments
    ///
    /// * `subject_id` - Identificador del nuevo sujeto preautorizado.
    /// * `providers` - Conjunto de identificadores de proveedores asociados.
    ///
    /// # Errors
    ///
    /// Devuelve un error si no se puede enviar un mensaje a través del canal de mensajes.
    pub async fn new_authorized_subject(
        &self,
        subject_id: DigestIdentifier,
        providers: HashSet<KeyIdentifier>,
    ) -> Result<(), AuthorizedSubjectsError> {
        if !providers.is_empty() {
            self.message_channel
                .tell(MessageTaskCommand::Request(
                    None,
                    TapleMessages::LedgerMessages(LedgerCommand::GetLCE {
                        who_asked: self.our_id.clone(),
                        subject_id,
                    }),
                    providers.into_iter().collect(),
                    MessageConfig::direct_response(),
                ))
                .await?;
        }
        Ok(())
    }
}
