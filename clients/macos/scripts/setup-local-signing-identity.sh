#!/bin/zsh
set -euo pipefail

IDENTITY_NAME="ChatOS Local Development"
LOGIN_KEYCHAIN=$(security default-keychain -d user | tr -d '"[:space:]')
SIGNING_DIRECTORY=${CHATOS_LOCAL_SIGNING_DIRECTORY:-"/Users/$(id -un)/Library/Application Support/ChatOSSwift/DevelopmentSigning"}
SIGNING_KEYCHAIN="$SIGNING_DIRECTORY/signing.keychain-db"
PASSWORD_FILE="$SIGNING_DIRECTORY/keychain-password"

if [[ -e "$SIGNING_KEYCHAIN" || -e "$PASSWORD_FILE" ]]; then
  if [[ ! -f "$SIGNING_KEYCHAIN" || ! -f "$PASSWORD_FILE" ]]; then
    print -u2 -- "The local signing state is incomplete: $SIGNING_DIRECTORY"
    exit 1
  fi
  signing_password=$(<"$PASSWORD_FILE")
  security unlock-keychain -p "$signing_password" "$SIGNING_KEYCHAIN"
  if security find-identity -v -p codesigning "$SIGNING_KEYCHAIN" 2>/dev/null \
    | grep -Fq "\"$IDENTITY_NAME\""; then
    security set-key-partition-list \
      -S apple-tool:,apple:,codesign: \
      -s \
      -k "$signing_password" \
      "$SIGNING_KEYCHAIN" \
      >/dev/null
    security lock-keychain "$SIGNING_KEYCHAIN"
    print -r -- "$IDENTITY_NAME is already installed."
    exit 0
  fi
  print -u2 -- "The local signing Keychain does not contain $IDENTITY_NAME."
  exit 1
fi

working_directory=$(mktemp -d /tmp/chatos-local-signing.XXXXXX)
key_path="$working_directory/signing-key.pem"
certificate_path="$working_directory/signing-certificate.pem"
archive_path="$working_directory/signing-identity.p12"
signing_password=$(openssl rand -hex 32)

mkdir -p "$SIGNING_DIRECTORY"
chmod 700 "$SIGNING_DIRECTORY"
umask 077
print -rn -- "$signing_password" >"$PASSWORD_FILE"

clean_up() {
  unlink "$key_path" 2>/dev/null || true
  unlink "$certificate_path" 2>/dev/null || true
  unlink "$archive_path" 2>/dev/null || true
  rmdir "$working_directory" 2>/dev/null || true
}
trap clean_up EXIT

openssl req \
  -x509 \
  -newkey rsa:3072 \
  -nodes \
  -keyout "$key_path" \
  -out "$certificate_path" \
  -days 3650 \
  -subj "/CN=$IDENTITY_NAME/O=ChatOS Local Development" \
  -addext "keyUsage=digitalSignature" \
  -addext "extendedKeyUsage=codeSigning" \
  >/dev/null 2>&1

openssl pkcs12 \
  -export \
  -legacy \
  -inkey "$key_path" \
  -in "$certificate_path" \
  -out "$archive_path" \
  -passout "pass:$signing_password" \
  >/dev/null 2>&1

security create-keychain -p "$signing_password" "$SIGNING_KEYCHAIN"
security unlock-keychain -p "$signing_password" "$SIGNING_KEYCHAIN"
security import "$archive_path" \
  -k "$SIGNING_KEYCHAIN" \
  -P "$signing_password" \
  -T /usr/bin/codesign \
  >/dev/null
security set-key-partition-list \
  -S apple-tool:,apple:,codesign: \
  -s \
  -k "$signing_password" \
  "$SIGNING_KEYCHAIN" \
  >/dev/null

print -r -- "macOS will ask once to trust the local development signing identity."
security add-trusted-cert \
  -r trustRoot \
  -p codeSign \
  -k "$LOGIN_KEYCHAIN" \
  "$certificate_path"

identity=$(security find-identity -v -p codesigning "$SIGNING_KEYCHAIN" \
  | awk '/"ChatOS Local Development"/{print $2; exit}')
if [[ -z "$identity" ]]; then
  print -u2 -- "The local signing identity was imported but is not trusted for code signing."
  exit 1
fi

print -r -- "$IDENTITY_NAME is ready."
