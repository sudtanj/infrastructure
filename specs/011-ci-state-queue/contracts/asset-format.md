# Contract: the `FCI1` asset format

Used for state files (kind 1) and artifact bundles (kind 2). Plaintext of an artifact is a tar.

```
offset  size  field
0       4     magic "FCI1"
4       1     kind: 0x01 state, 0x02 artifact
5       16    salt, random per file
21      ...   chunks
```

Key: `HKDF-SHA256(salt = file salt, ikm = the decoded bytes of the key setting (base64 or hex text decoding to 32 bytes or more), info = "factory-ci/v1/state" or "factory-ci/v1/artifact")`, 32 bytes. The setting is `FACTORY_CI_STATE_KEY` for kind 1 and `FACTORY_CI_ARTIFACT_KEY` for kind 2.

Chunks: each holds up to 65,536 plaintext bytes sealed with ChaCha20-Poly1305 (16-byte tag appended). Every chunk but the last holds exactly 65,536 plaintext bytes. The last chunk holds 0 to 65,536 bytes, so an empty plaintext is one 16-byte chunk.

Nonce (12 bytes): bytes 0 to 2 zero, bytes 3 to 10 the chunk index as a big-endian u64, byte 11 is `0x01` for the last chunk and `0x00` otherwise.

Associated data: the 21 header bytes, for every chunk.

`open` fails on a wrong magic, a wrong kind for the key, a failed tag, a missing last chunk, bytes after the last chunk, or a short read. It reads one byte ahead to know the last chunk, so a plaintext of exactly 65,536 bytes has one valid encoding. It releases plaintext only for chunks that verified.

## Decrypt for an agent

Use `rust-ci-runner/scripts/open-artifact.sh` (T028). It downloads the runner binary of the release and refuses to run it unless its sha256 equals the value pinned in the script, decrypts with `decrypt-artifact`, checks the exit status before extracting, extracts with `tar -x --no-same-owner` into a new empty directory, and prints files with control characters and ANSI removed, capped in size, under a header that marks them untrusted data. The key goes in the environment, never argv. A truncated stream gives a valid prefix before the error; the exit status is the signal.

The agent holds the artifact key, a Loki read token limited to this tenant and step `ci`, and a GitHub token with contents read on the store repo only. It has no write tools and egress to GitHub and Loki only.
