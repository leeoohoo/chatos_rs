#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

set -euo pipefail

OUTPUT_DIR="${1:-}"
if [[ -z "$OUTPUT_DIR" ]]; then
  echo "Usage: $0 <output-directory>" >&2
  exit 2
fi
command -v openssl >/dev/null 2>&1 || {
  echo "[ERROR] openssl is required to generate User Service mTLS material" >&2
  exit 1
}

CALLERS=(project-service)

material_is_current() {
  [[ -f "$OUTPUT_DIR/ca.crt" && -f "$OUTPUT_DIR/server.crt" && -f "$OUTPUT_DIR/server.key" ]] || return 1
  openssl verify -purpose sslserver -CAfile "$OUTPUT_DIR/ca.crt" "$OUTPUT_DIR/server.crt" >/dev/null 2>&1 || return 1
  openssl pkey -in "$OUTPUT_DIR/server.key" -noout >/dev/null 2>&1 || return 1
  local caller
  for caller in "${CALLERS[@]}"; do
    [[ -f "$OUTPUT_DIR/${caller}.identity.pem" ]] || return 1
    openssl verify -purpose sslclient -CAfile "$OUTPUT_DIR/ca.crt" "$OUTPUT_DIR/${caller}.identity.pem" >/dev/null 2>&1 || return 1
    openssl pkey -in "$OUTPUT_DIR/${caller}.identity.pem" -noout >/dev/null 2>&1 || return 1
  done
}

mkdir -p "$OUTPUT_DIR"
if material_is_current; then
  echo "[INFO] User Service mTLS material is current: $OUTPUT_DIR"
  exit 0
fi

temporary_dir="$(mktemp -d "${OUTPUT_DIR%/}/.generate.XXXXXX")"
trap 'rm -rf "$temporary_dir"' EXIT

openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 -out "$temporary_dir/ca.key" >/dev/null 2>&1
openssl req -x509 -new -sha256 -days 3650 -key "$temporary_dir/ca.key" \
  -subj "/CN=ChatOS User Service Internal CA" -out "$temporary_dir/ca.crt"
openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$temporary_dir/server.key" >/dev/null 2>&1
openssl req -new -sha256 -key "$temporary_dir/server.key" \
  -subj "/CN=user-service-backend" -out "$temporary_dir/server.csr"
cat >"$temporary_dir/server.ext" <<'EOF'
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=DNS:user-service-backend,DNS:user-service,DNS:localhost,IP:127.0.0.1
EOF
openssl x509 -req -sha256 -days 825 -in "$temporary_dir/server.csr" \
  -CA "$temporary_dir/ca.crt" -CAkey "$temporary_dir/ca.key" -CAcreateserial \
  -extfile "$temporary_dir/server.ext" -out "$temporary_dir/server.crt" >/dev/null 2>&1

for caller in "${CALLERS[@]}"; do
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 -out "$temporary_dir/${caller}.key" >/dev/null 2>&1
  openssl req -new -sha256 -key "$temporary_dir/${caller}.key" -subj "/CN=${caller}" -out "$temporary_dir/${caller}.csr"
  cat >"$temporary_dir/${caller}.ext" <<EOF
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=clientAuth
subjectAltName=DNS:${caller}
EOF
  openssl x509 -req -sha256 -days 825 -in "$temporary_dir/${caller}.csr" \
    -CA "$temporary_dir/ca.crt" -CAkey "$temporary_dir/ca.key" -CAcreateserial \
    -extfile "$temporary_dir/${caller}.ext" -out "$temporary_dir/${caller}.crt" >/dev/null 2>&1
  cat "$temporary_dir/${caller}.crt" "$temporary_dir/${caller}.key" >"$temporary_dir/${caller}.identity.pem"
done

install -m 0644 "$temporary_dir/ca.crt" "$OUTPUT_DIR/ca.crt"
install -m 0644 "$temporary_dir/server.crt" "$OUTPUT_DIR/server.crt"
install -m 0600 "$temporary_dir/server.key" "$OUTPUT_DIR/server.key"
install -m 0600 "$temporary_dir/ca.key" "$OUTPUT_DIR/ca.key"
for caller in "${CALLERS[@]}"; do
  install -m 0600 "$temporary_dir/${caller}.identity.pem" "$OUTPUT_DIR/${caller}.identity.pem"
done

echo "[INFO] generated User Service mTLS material: $OUTPUT_DIR"
