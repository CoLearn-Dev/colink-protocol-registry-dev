1.
```
bash install_colink.sh
```
2.
```
colinkctl enable_dev_env
```
3. Start the protocol (get user jwt from user_token.txt)
```
e.g. cargo run -- --addr http://127.0.0.1:8080 --jwt jwt
```
4. Run app code example
```
e.g. cargo run --example update_registries http://127.0.0.1:8080 user_jwt registry_jwt
```
