#!/bin/zsh
set -euo pipefail

SCRIPT_DIR=${0:A:h}
PROJECT_DIR=${SCRIPT_DIR:h}

cd "$PROJECT_DIR"

# SwiftUI string-literal initializers participate in Bundle localization. This
# report finds visible Chinese literals that still need an English entry. User
# content, paths, commands and backend payloads must not be translated.
ruby <<'RUBY'
files = Dir["Sources/ChatOSApp/**/*.swift"]
pattern = /(?:Text|Label|Button|Toggle|Picker|GroupBox|ProgressView|navigationTitle|help|ContentUnavailableView|TextField|SecureField|Section)\s*\(\s*"((?:[^"\\]|\\.)*[一-龥](?:[^"\\]|\\.)*)"/m
strings = files.flat_map do |file|
  File.read(file).scan(pattern).map { |match| match.first.gsub("\\n", " ") }
end.reject { |value| value.include?("\\(") }.uniq.sort

catalog_path = "Support/Localization/en.lproj/Localizable.strings"
catalog = File.exist?(catalog_path) ? File.read(catalog_path) : ""
missing = strings.reject do |value|
  escaped = value.gsub("\\", "\\\\").gsub('"', '\\"')
  catalog.include?(%Q{"#{escaped}" =})
end

puts "UI Chinese literals: #{strings.count}"
puts "Missing English entries: #{missing.count}"
missing.each { |value| puts value }

def catalog_pairs(path)
  return {} unless File.exist?(path)
  File.read(path).scan(/"((?:[^"\\]|\\.)*)"\s*=\s*"((?:[^"\\]|\\.)*)";/).to_h
end

english_pairs = catalog_pairs(catalog_path)
chinese_pairs = catalog_pairs("Support/Localization/zh-Hans.lproj/Localizable.strings")
invalid_chinese = english_pairs.keys.reject { |key| chinese_pairs[key] == key }

puts "Chinese identity entries: #{chinese_pairs.count}"
puts "Missing or non-identity Chinese entries: #{invalid_chinese.count}"
invalid_chinese.each { |value| puts value }
exit(missing.empty? && invalid_chinese.empty? ? 0 : 1)
RUBY
