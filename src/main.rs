#![allow(clippy::derive_partial_eq_without_eq)]
#![allow(clippy::uninlined_format_args)]
use colink::{
    decode_jwt_without_validation, utils::get_colink_home, CoLink, Participant, ProtocolEntry,
};
use colink_registry_proto::*;
use fs4::FileExt;
use prost::Message;
use std::{
    fs::File,
    io::{Read, Seek, Write},
    path::Path,
};

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
        let colink_home = get_colink_home()?;
        let registry_file = Path::new(&colink_home).join("reg_config");
        let registry_addr;
        let registry_jwt;
        if let Ok(mut file) = File::options().read(true).write(true).open(&registry_file) {
            file.lock_exclusive()?;
            let mut buf = String::new();
            file.read_to_string(&mut buf)?;
            let lines: Vec<&str> = buf.lines().collect();
            if lines.is_empty() || lines[0].is_empty() {
                registry_addr = cl.get_core_addr()?;
                registry_jwt = cl.generate_token("guest").await?;
                file.rewind()?;
                file.write_all(format!("{}\n{}\n", registry_addr, registry_jwt).as_bytes())?;
            } else {
                registry_addr = lines[0].to_string();
                registry_jwt = lines[1].to_string();
            }
            file.unlock()?;
        } else {
            registry_addr = String::from_utf8_lossy(
                &cl.read_entry("_registry:init:default_registry_addr")
                    .await?,
            )
            .to_string();
            registry_jwt = String::from_utf8_lossy(
                &cl.read_entry("_registry:init:default_registry_jwt").await?,
            )
            .to_string();
        }
        let registry = Registry {
            address: registry_addr,
            guest_jwt: registry_jwt,
        };
        let registries = Registries {
            registries: vec![registry],
        };
        update_registries(&cl, &registries).await?;
        Ok(())
    }
}

async fn update_registries(
    cl: &CoLink,
    registries: &Registries,
) -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let mut payload = vec![];
    registries.encode(&mut payload).unwrap();
    cl.update_entry("_registry:registries", &payload).await?;
    let guest_jwt = cl
        .generate_token_with_expiration_time(chrono::Utc::now().timestamp() + 86400 * 31, "guest")
        .await?;
    let user_record = UserRecord {
        user_id: cl.get_user_id()?,
        core_addr: cl.get_core_addr()?,
        guest_jwt,
    };
    let mut payload = vec![];
    user_record.encode(&mut payload).unwrap();
    for registry in &registries.registries {
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
        update_registries(&cl, &registries).await?;
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
        for _ in 0..3 {
            for registry in &registries.registries {
                let user_id = decode_jwt_without_validation(&registry.guest_jwt)?.user_id;
                let data = if user_id == cl.get_user_id()? {
                    cl.read_entry(&format!(
                        "_remote_storage:public:{}:_registry:user_record",
                        user_record.user_id
                    ))
                    .await
                } else {
                    cl.remote_storage_read(
                        &user_id,
                        "_registry:user_record",
                        true,
                        &user_record.user_id,
                    )
                    .await
                };
                if data.is_ok() {
                    let user_record: UserRecord = Message::decode(&*data.unwrap())?;
                    cl.import_guest_jwt(&user_record.guest_jwt).await?;
                    cl.import_core_addr(&user_record.user_id, &user_record.core_addr)
                        .await?;
                    return Ok(());
                }
            }
            tokio::time::sleep(core::time::Duration::from_secs(1)).await;
        }
        Ok(())
    }
}

colink::protocol_start!(
    ("registry:@init", Init),
    ("registry:update_registries", UpdateRegistries),
    ("registry:query_from_registries", QueryFromRegistries)
);
