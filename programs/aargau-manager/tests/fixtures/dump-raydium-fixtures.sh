#!/bin/sh
# Re-dump the external programs and mainnet account snapshots used by the
# Raydium CLMM mollusk replay. Needs only curl, jq, base64 and coreutils.
#
#   sh dump-raydium-fixtures.sh programs
#       Dump the program ELFs into this directory (gitignored) and check each
#       against its pinned hash. A mismatch means the program was upgraded
#       upstream: the file is kept for inspection, the script exits non-zero,
#       and the pins in fixtures/mod.rs must be reviewed before use.
#
#   sh dump-raydium-fixtures.sh snapshots <pool-dir>
#       Re-capture every account listed in <pool-dir>/manifest.txt in ONE
#       getMultipleAccounts call, so all snapshots share a slot. Overwrites the
#       committed <label>.json files: re-check the replay ranges afterwards.
#
# RPC defaults to the public mainnet endpoint; override with RPC=<url>.

set -eu

RPC="${RPC:-https://api.mainnet-beta.solana.com}"
FIXTURES_DIR=$(cd "$(dirname "$0")" && pwd)

# Bytes before the ELF in an upgradeable-loader ProgramData account:
# 4 (enum tag) + 8 (deploy slot) + 1 (Option tag) + 32 (upgrade authority).
PROGRAM_DATA_HEADER_LEN=45

rpc() {
    curl -sS --fail --retry 3 --retry-delay 2 "$RPC" \
        -H 'content-type: application/json' -d "$1"
}

account_info() {
    # $1 = pubkey, $2 = encoding
    rpc "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getAccountInfo\",\"params\":[\"$1\",{\"encoding\":\"$2\"}]}"
}

# sha256 of the file with trailing zero bytes stripped: ProgramData accounts
# are zero-padded past the ELF, and the padding changes with every resize.
stripped_sha256() {
    last_nonzero=$(od -An -v -tx1 "$1" | tr -s ' ' '\n' | grep -v '^$' \
        | grep -n -v '^00$' | tail -n 1 | cut -d: -f1)
    head -c "$last_nonzero" "$1" | sha256sum | cut -d' ' -f1
}

check_pin() {
    # $1 = file, $2 = expected stripped sha256
    raw=$(sha256sum "$1" | cut -d' ' -f1)
    stripped=$(stripped_sha256 "$1")
    echo "  raw sha256:      $raw"
    echo "  stripped sha256: $stripped"
    if [ "$stripped" != "$2" ]; then
        echo "  PIN MISMATCH: expected $2 (program upgraded upstream?)" >&2
        return 1
    fi
    echo "  pin: ok"
}

dump_upgradeable() {
    # $1 = program id, $2 = output file name, $3 = pinned stripped sha256
    out="$FIXTURES_DIR/$2"
    program_data=$(account_info "$1" jsonParsed | jq -er '.result.value.data.parsed.info.programData')
    response=$(account_info "$program_data" base64)
    echo "$response" | jq -er '.result.value.data[0]' | base64 -d > "$out.programdata"
    deploy_slot=$(od -An -tu8 -j4 -N8 "$out.programdata" | tr -d ' ')
    tail -c +$((PROGRAM_DATA_HEADER_LEN + 1)) "$out.programdata" > "$out"
    rm -f "$out.programdata"
    echo "$2: program $1, ProgramData $program_data, deploy slot $deploy_slot"
    check_pin "$out" "$3"
}

dump_loader_v2() {
    # $1 = program id, $2 = output file name, $3 = pinned stripped sha256
    out="$FIXTURES_DIR/$2"
    owner=$(account_info "$1" base64 | tee "$out.account" | jq -er '.result.value.owner')
    if [ "$owner" != "BPFLoader2111111111111111111111111111111111" ]; then
        echo "$2: expected BPF Loader 2 owner, got $owner" >&2
        rm -f "$out.account"
        return 1
    fi
    jq -er '.result.value.data[0]' "$out.account" | base64 -d > "$out"
    rm -f "$out.account"
    echo "$2: program $1 (BPF Loader 2, immutable)"
    check_pin "$out" "$3"
}

dump_programs() {
    status=0
    dump_upgradeable CAMMCzo5YL8w4VFF8KVHrK22GGUsp5VTaW7grrKgrWqK raydium_clmm.so \
        f78e9dbc080068facc531ab4b679680dd39efb9d07a08716b37d493e99272fc9 || status=1
    dump_upgradeable TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA spl_token.so \
        435df1e70f1ca6258eb01a4507701688c7f1a22656e3e4ef361db473bf478075 || status=1
    dump_upgradeable TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb spl_token_2022.so \
        7ea94a027005b39196fa08e6d0ddcd55ec32eb43266179921a834a1ba2eb1947 || status=1
    dump_loader_v2 ATokenGPvbdGVxr1b2hvZbsiqW5xWH25efTNsLJA8knL spl_associated_token_account.so \
        e93ab1ff110697630b0814bd49980a3b85f8890fa5a574c439eace3120522b5c || status=1
    dump_loader_v2 MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr spl_memo.so \
        68d9bbae7023f8f51d32d0f6ff8a7003921c97c99e77921ccd0f51adcf6295db || status=1
    return $status
}

dump_snapshots() {
    pool_dir=$1
    manifest="$pool_dir/manifest.txt"
    [ -f "$manifest" ] || { echo "missing $manifest" >&2; return 1; }

    keys=$(grep -v '^#' "$manifest" | grep -v '^[[:space:]]*$' \
        | awk '{ printf "%s\"%s\"", (NR > 1 ? "," : ""), $2 }')
    response="$pool_dir/.response.json"
    rpc "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"getMultipleAccounts\",\"params\":[[$keys],{\"encoding\":\"base64\"}]}" \
        > "$response"
    jq -e '.result.value' "$response" > /dev/null

    index=0
    grep -v '^#' "$manifest" | grep -v '^[[:space:]]*$' | while read -r label pubkey; do
        jq -e --argjson i "$index" --arg pubkey "$pubkey" '
            .result.context.slot as $slot
            | .result.value[$i]
            | if . == null then error("account \($pubkey) does not exist") else . end
            | { slot: $slot, pubkey: $pubkey, owner: .owner, lamports: .lamports,
                executable: .executable, data_base64: .data[0] }' \
            "$response" > "$pool_dir/$label.json"
        index=$((index + 1))
    done
    slot=$(jq -r '.result.context.slot' "$response")
    rm -f "$response"
    echo "$pool_dir: snapshot slot $slot"
}

case "${1:-}" in
    programs) dump_programs ;;
    snapshots) dump_snapshots "${2:?usage: $0 snapshots <pool-dir>}" ;;
    *) echo "usage: $0 programs | snapshots <pool-dir>" >&2; exit 2 ;;
esac
