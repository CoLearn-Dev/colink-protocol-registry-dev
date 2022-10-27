use colink::*;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync + 'static>> {
    let args = env::args().skip(1).collect::<Vec<_>>();
    let addr = &args[0];
    let host_jwt = &args[1];
    let expiration_timestamp: i64 = if args.len() > 2 {
        args[2].parse().unwrap()
    } else {
        // 1 year
        chrono::Utc::now().timestamp() + 86400 * 365
    };

    let cl = CoLink::new(addr, host_jwt);
    let sk = secp256k1::SecretKey::from_slice(&hex::decode(
        "e83b334f83311761e63ca8bee06a45f8b580cc270fef353577882b0140d3e30f",
    )?)?;
    let pk = secp256k1::PublicKey::from_secret_key(&secp256k1::Secp256k1::new(), &sk);
    let (_, core_pub_key, _) = cl.request_info().await?;
    let (signature_timestamp, sig) =
        prepare_import_user_signature(&pk, &sk, &core_pub_key, expiration_timestamp);
    let user_jwt = cl
        .import_user(&pk, signature_timestamp, expiration_timestamp, &sig)
        .await?;
    let cl_user = CoLink::new(addr, &user_jwt);
    let guest_jwt = cl_user
        .generate_token_with_expiration_time(expiration_timestamp, "guest")
        .await?;
    println!("user_jwt: {}", user_jwt);
    println!("guest_jwt: {}", guest_jwt);

    Ok(())
}
