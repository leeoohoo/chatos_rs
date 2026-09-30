import Foundation

extension String {
    var urlPathEncoded: String {
        let segmentAllowed = CharacterSet.urlPathAllowed
            .subtracting(CharacterSet(charactersIn: "/"))
        return addingPercentEncoding(withAllowedCharacters: segmentAllowed) ?? self
    }
}
