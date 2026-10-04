#!/usr/bin/env bash
# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

generate_service_mtls() {
  local output_dir="${1:-}"
  local service_name="$2"
  local ca_common_name="$3"
  local server_common_name="$4"
  local server_subject_alt_names="$5"
  shift 5
  local callers=("$@")

  if [[ -z "$output_dir" ]]; then
    echo "Usage: $0 <output-directory>" >&2
    return 2
  fi
  if ! command -v openssl >/dev/null 2>&1; then
    echo "[ERROR] openssl is required to generate ${service_name} mTLS material" >&2
    return 1
  fi

  service_mtls_material_is_current() {
    [[ -f "$output_dir/ca.crt" && -f "$output_dir/server.crt" && -f "$output_dir/server.key" ]] || return 1
    openssl x509 -checkend 86400 -noout -in "$output_dir/ca.crt" >/dev/null 2>&1 || return 1
    openssl x509 -checkend 86400 -noout -in "$output_dir/server.crt" >/dev/null 2>&1 || return 1
    openssl pkey -in "$output_dir/server.key" -noout >/dev/null 2>&1 || return 1
    openssl verify -purpose sslserver -CAfile "$output_dir/ca.crt" "$output_dir/server.crt" >/dev/null 2>&1 || return 1
    local caller
    for caller in "${callers[@]}"; do
      [[ -f "$output_dir/${caller}.identity.pem" ]] || return 1
      openssl x509 -checkend 86400 -noout -in "$output_dir/${caller}.identity.pem" >/dev/null 2>&1 || return 1
      openssl pkey -in "$output_dir/${caller}.identity.pem" -noout >/dev/null 2>&1 || return 1
      openssl verify -purpose sslclient -CAfile "$output_dir/ca.crt" \
        "$output_dir/${caller}.identity.pem" >/dev/null 2>&1 || return 1
    done
  }

  mkdir -p "$output_dir"
  if service_mtls_material_is_current; then
    echo "[INFO] ${service_name} mTLS material is current: $output_dir"
    return 0
  fi

  local temporary_dir
  temporary_dir="$(mktemp -d "${output_dir%/}/.generate.XXXXXX")"
  SERVICE_MTLS_TEMPORARY_DIR="$temporary_dir"
  trap 'rm -rf "$SERVICE_MTLS_TEMPORARY_DIR"' EXIT

  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:3072 \
    -out "$temporary_dir/ca.key" >/dev/null 2>&1
  openssl req -x509 -new -sha256 -days 3650 \
    -key "$temporary_dir/ca.key" -subj "/CN=${ca_common_name}" \
    -out "$temporary_dir/ca.crt"
  openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
    -out "$temporary_dir/server.key" >/dev/null 2>&1
  openssl req -new -sha256 -key "$temporary_dir/server.key" \
    -subj "/CN=${server_common_name}" -out "$temporary_dir/server.csr"
  cat >"$temporary_dir/server.ext" <<EOF
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=serverAuth
subjectAltName=${server_subject_alt_names}
EOF
  openssl x509 -req -sha256 -days 825 -in "$temporary_dir/server.csr" \
    -CA "$temporary_dir/ca.crt" -CAkey "$temporary_dir/ca.key" -CAcreateserial \
    -extfile "$temporary_dir/server.ext" -out "$temporary_dir/server.crt" >/dev/null 2>&1

  local caller
  for caller in "${callers[@]}"; do
    openssl genpkey -algorithm RSA -pkeyopt rsa_keygen_bits:2048 \
      -out "$temporary_dir/${caller}.key" >/dev/null 2>&1
    openssl req -new -sha256 -key "$temporary_dir/${caller}.key" \
      -subj "/CN=${caller}" -out "$temporary_dir/${caller}.csr"
    cat >"$temporary_dir/${caller}.ext" <<EOF
basicConstraints=critical,CA:FALSE
keyUsage=critical,digitalSignature,keyEncipherment
extendedKeyUsage=clientAuth
subjectAltName=DNS:${caller}
EOF
    openssl x509 -req -sha256 -days 825 -in "$temporary_dir/${caller}.csr" \
      -CA "$temporary_dir/ca.crt" -CAkey "$temporary_dir/ca.key" -CAcreateserial \
      -extfile "$temporary_dir/${caller}.ext" -out "$temporary_dir/${caller}.crt" >/dev/null 2>&1
    cat "$temporary_dir/${caller}.crt" "$temporary_dir/${caller}.key" \
      >"$temporary_dir/${caller}.identity.pem"
  done

  install -m 0644 "$temporary_dir/ca.crt" "$output_dir/ca.crt"
  install -m 0644 "$temporary_dir/server.crt" "$output_dir/server.crt"
  install -m 0600 "$temporary_dir/server.key" "$output_dir/server.key"
  install -m 0600 "$temporary_dir/ca.key" "$output_dir/ca.key"
  for caller in "${callers[@]}"; do
    install -m 0600 "$temporary_dir/${caller}.identity.pem" \
      "$output_dir/${caller}.identity.pem"
  done

  echo "[INFO] generated ${service_name} mTLS material: $output_dir"
}
