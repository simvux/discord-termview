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

### Bugs: 

 * Terminals that have existed before cannot be recreated? 
 * Messages that end to quickly fail to ever attach their output to a frame
 * It's really slow, even error messages take a long time. Is it because they're delayed form rate limitor due to frame updates of terminals?
 * Some commands like `xbps-query -Rs ` can just completely lock up the entire pipeline
 * Creation of new terminal breaks as soon as any command has finished it's execution
     any `send_message` or `reply` calls to discord seem to just freeze.
