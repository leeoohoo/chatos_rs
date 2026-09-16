const nacl = require('tweetnacl')

exports.publicKeyFromSeed = function publicKeyFromSeed(seed) {
  return nacl.sign.keyPair.fromSeed(seed).publicKey
}

exports.signDetached = function signDetached(message, seed) {
  const keyPair = nacl.sign.keyPair.fromSeed(seed)
  try {
    return nacl.sign.detached(message, keyPair.secretKey)
  } finally {
    keyPair.secretKey.fill(0)
  }
}

exports.sha512 = function sha512(message) {
  return nacl.hash(message)
}
