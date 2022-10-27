#![allow(clippy::derive_partial_eq_without_eq)]
use colink::{decode_jwt_without_validation, CoLink, Participant, ProtocolEntry};
use colink_registry_proto::*;
use prost::Message;

mod colink_registry_proto {
    include!(concat!(env!("OUT_DIR"), "/colink_registry.rs"));
}

pub struct Init;
#[colink::async_trait]
impl ProtocolEntry for Init {
    async fn start(
        &self,
        cl: CoLink,
        _param: Vec<u8>,
        _participants: Vec<Participant>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let registry_addr =
            String::from_utf8_lossy(&cl.read_entry("_registry:init:registry_addr").await?)
                .to_string();
        let registry_jwt =
            String::from_utf8_lossy(&cl.read_entry("_registry:init:registry_jwt").await?)
                .to_string();
        let registry = colink::extensions::registry::Registry {
            address: registry_addr,
            guest_jwt: registry_jwt,
        };
        let registries = colink::extensions::registry::Registries {
            registries: vec![registry],
        };
        cl.update_registries(&registries).await?;
        Ok(())
    }
}

struct UpdateRegistries;
#[colink::async_trait]
impl ProtocolEntry for UpdateRegistries {
    async fn start(
        &self,
        cl: CoLink,
        param: Vec<u8>,
        _participants: Vec<Participant>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let _ = async {
            let registries = cl.read_entry("_registry:registries").await?;
            let registries: Registries = Message::decode(&*registries)?;
            for registry in registries.registries {
                let _ = async {
                    let user_id = decode_jwt_without_validation(&registry.guest_jwt)?.user_id;
                    if user_id == cl.get_user_id()? {
                        cl.delete_entry(&format!(
                            "_remote_storage:public:{}:_registry:user_record",
                            user_id,
                        ))
                        .await?;
                    } else {
                        cl.import_guest_jwt(&registry.guest_jwt).await?;
                        cl.import_core_addr(&user_id, &registry.address).await?;
                        cl.remote_storage_delete(&[user_id], "_registry:user_record", true)
                            .await?;
                    }
                    Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
                }
                .await;
            }
            Ok::<(), Box<dyn std::error::Error + Send + Sync + 'static>>(())
        }
        .await;
        let registries: Registries = Message::decode(&*param)?;
        cl.update_entry("_registry:registries", &param).await?;
        let guest_jwt = cl
            .generate_token_with_expiration_time(
                chrono::Utc::now().timestamp() + 86400 * 31,
                "guest",
            )
            .await?;
        let user_record = UserRecord {
            user_id: cl.get_user_id()?,
            core_addr: cl.get_core_addr()?,
            guest_jwt,
        };
        let mut payload = vec![];
        user_record.encode(&mut payload).unwrap();
        for registry in registries.registries {
            let user_id = decode_jwt_without_validation(&registry.guest_jwt)?.user_id;
            if user_id == cl.get_user_id()? {
                cl.update_entry(
                    &format!("_remote_storage:public:{}:_registry:user_record", user_id,),
                    &payload,
                )
                .await?;
            } else {
                cl.import_guest_jwt(&registry.guest_jwt).await?;
                cl.import_core_addr(&user_id, &registry.address).await?;
                cl.remote_storage_update(&[user_id], "_registry:user_record", &payload, true)
                    .await?;
            }
        }
        Ok(())
    }
}

struct QueryFromRegistries;
#[colink::async_trait]
impl ProtocolEntry for QueryFromRegistries {
    async fn start(
        &self,
        cl: CoLink,
        param: Vec<u8>,
        _participants: Vec<Participant>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let user_record: UserRecord = Message::decode(&*param)?;
        let registries = cl.read_entry("_registry:registries").await?;
        let registries: Registries = Message::decode(&*registries)?;
        for registry in registries.registries {
            let user_id = decode_jwt_without_validation(&registry.guest_jwt)?.user_id;
            if user_id == cl.get_user_id()? {
                let data = cl
                    .read_entry(&format!(
                        "_remote_storage:public:{}:_registry:user_record",
                        user_record.user_id
                    ))
                    .await?;
                let user_record: UserRecord = Message::decode(&*data)?;
                cl.import_guest_jwt(&user_record.guest_jwt).await?;
                cl.import_core_addr(&user_record.user_id, &user_record.core_addr)
                    .await?;
                return Ok(());
            } else if let Ok(data) = cl
                .remote_storage_read(
                    &user_id,
                    "_registry:user_record",
                    true,
                    &user_record.user_id,
                )
                .await
            {
                let user_record: UserRecord = Message::decode(&*data)?;
                cl.import_guest_jwt(&user_record.guest_jwt).await?;
                cl.import_core_addr(&user_record.user_id, &user_record.core_addr)
                    .await?;
                return Ok(());
            }
        }
        Ok(())
    }
}

colink::protocol_start!(
    ("registry:@init", Init),
    ("registry:update_registries", UpdateRegistries),
    ("registry:query_from_registries", QueryFromRegistries)
);
