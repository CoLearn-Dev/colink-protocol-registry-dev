use colink_registry_proto::*;
use colink_remote_storage_proto::*;
use colink_sdk::{decode_jwt_without_validation, CoLink, Participant, ProtocolEntry};
use prost::Message;

mod colink_registry_proto {
    include!(concat!(env!("OUT_DIR"), "/colink_registry.rs"));
}
mod colink_remote_storage_proto {
    include!(concat!(env!("OUT_DIR"), "/colink_remote_storage.rs"));
}

struct SetRegistries;
#[colink_sdk::async_trait]
impl ProtocolEntry for SetRegistries {
    async fn start(
        &self,
        cl: CoLink,
        param: Vec<u8>,
        _participants: Vec<Participant>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
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
            core_addr: "http://127.0.0.1:8080".to_string(), // TODO cl.get_core_addr
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
                let participants = vec![
                    Participant {
                        user_id: cl.get_user_id()?,
                        role: "requester".to_string(),
                    },
                    Participant {
                        user_id,
                        role: "provider".to_string(),
                    },
                ];
                let params = UpdateParams {
                    remote_key_name: "_registry:user_record".to_string(),
                    payload: payload.clone(),
                    is_public: true,
                };
                let mut payload = vec![];
                params.encode(&mut payload).unwrap();
                cl.run_task("remote_storage.update", &payload, &participants, false)
                    .await?;
            }
        }
        Ok(())
    }
}

struct Query;
#[colink_sdk::async_trait]
impl ProtocolEntry for Query {
    async fn start(
        &self,
        cl: CoLink,
        param: Vec<u8>,
        _participants: Vec<Participant>,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
        let user_record: UserRecord = Message::decode(&*param)?;
        let registries = cl.read_entry("_registry:registries").await?;
        let registries: Registries = Message::decode(&*registries)?;
        #[allow(clippy::never_loop)]
        for registry in registries.registries {
            let user_id = decode_jwt_without_validation(&registry.guest_jwt)?.user_id;
            if user_id == cl.get_user_id()? {
                let data = cl
                    .read_entry(&format!(
                        "_remote_storage:public:{}:_registry:user_record",
                        user_record.user_id
                    ))
                    .await?; // TODO error?
                let user_record: UserRecord = Message::decode(&*data)?;
                cl.import_guest_jwt(&user_record.guest_jwt).await?;
                cl.import_core_addr(&user_record.user_id, &user_record.core_addr)
                    .await?;
                return Ok(());
            } else {
                let participants = vec![
                    Participant {
                        user_id: cl.get_user_id()?,
                        role: "requester".to_string(),
                    },
                    Participant {
                        user_id,
                        role: "provider".to_string(),
                    },
                ];
                let params = ReadParams {
                    remote_key_name: "_registry:user_record".to_string(),
                    holder_id: user_record.user_id.clone(),
                    is_public: true,
                };
                let mut payload = vec![];
                params.encode(&mut payload).unwrap();
                let task_id = cl
                    .run_task("remote_storage.read", &payload, &participants, false)
                    .await?;
                let data = cl
                    .read_or_wait(&format!("tasks:{}:output", task_id))
                    .await?; // TODO timeout?
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

colink_sdk::protocol_start!(
    ("registry:set_registries", SetRegistries),
    ("registry:query", Query)
);
