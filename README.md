# Termview

## WARNING

Using this bot can be exceedingly dangerous since you're basically granting people direct access to your shell. 

Make sure you know what you're doing! 

## Running

Make sure you have cargo installed (might need nightly, but probably doesn't)

### Locally (dangerous)

```
# Compile
cargo build --release

# Configure
export DISCORD_TOKEN=my-discord-token
export ALLOWED_ROLES=<id-of-role>

# Run
target/release/discord-termview
```

### In docker container (slightly less dangerous)

Edit `build.sh` with your token and role-id

`./build.sh run`

## TODO

 * make sessions automatically expire (difficult)
 * make the terminal move/repost if it's to far away (moderate)
 * allow killing terminals (difficult)
 * allow multi-line code blocks (easy)
 * help message (easy)
