use colink_sdk::*;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let addr = &args[0];
    let user_jwt = &args[1];
    let registry_jwt = &args[2];

    let registry = Registry {
        address: addr.clone(),
        guest_jwt: registry_jwt,
    };
    let registries = Registries {
        registries: vec![registry],
    };
    let cl = CoLink::new(addr, user_jwt);
    cl.update_registries(&registries).await?;

    Ok(())
}
