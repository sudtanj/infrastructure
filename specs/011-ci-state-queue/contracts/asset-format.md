# Contract: the `FCI1` asset format

Used for state files (kind 1) and artifact bundles (kind 2). Plaintext of an artifact is a tar.

```
offset  size  field
0       4     magic "FCI1"
4       1     kind: 0x01 state, 0x02 artifact
5       16    salt, random per file
21      ...   chunks
```

Key: `HKDF-SHA256(salt = file salt, ikm = the key setting as UTF-8, info = "factory-ci/v1/state" or "factory-ci/v1/artifact")`, 32 bytes. The setting is `FACTORY_CI_STATE_KEY` for kind 1 and `FACTORY_CI_ARTIFACT_KEY` for kind 2.

Chunks: each holds up to 65,536 plaintext bytes sealed with ChaCha20-Poly1305 (16-byte tag appended). Every chunk but the last holds exactly 65,536 plaintext bytes. The last chunk holds 0 to 65,536 bytes, so an empty plaintext is one 16-byte chunk.

Nonce (12 bytes): bytes 0 to 2 zero, bytes 3 to 10 the chunk index as a big-endian u64, byte 11 is `0x01` for the last chunk and `0x00` otherwise.

Associated data: the 21 header bytes, for every chunk.

`open` fails on a wrong magic, a wrong kind for the key, a failed tag, a missing last chunk, bytes after the last chunk, or a short read. It releases plaintext only for chunks that verified.

## Decrypt for an agent

The agent holds the artifact key, a Loki read token and a GitHub token with contents read on the one private repo. Values come from the agent's own secret store; the key goes in the environment, never argv.

```sh
# once: the runner binary from this public repo's release (x86_64 shown)
gh release download rust-ci-runner-v0.4.0 --repo sudtanj/infrastructure \
  --pattern rust-ci-runner-x86_64-linux --dir ./bin && chmod +x ./bin/rust-ci-runner-x86_64-linux

# per bundle: release and asset names come from the Loki line
set -o pipefail
gh release download "$RELEASE" --repo "$REPO" --pattern "$ASSET" -O - \
  | FACTORY_CI_ARTIFACT_KEY="$KEY" ./bin/rust-ci-runner-x86_64-linux decrypt-artifact \
  | tar -x -C ./artifact
```

Check the exit status before trusting the files.
