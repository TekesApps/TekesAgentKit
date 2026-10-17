import Foundation
let cases = try JSONSerialization.jsonObject(with: Data(contentsOf: URL(fileURLWithPath: CommandLine.arguments[1]))) as! [[String: Any]]
let decoder = JSONDecoder()
for item in cases {
  let data = try JSONSerialization.data(withJSONObject: item["value"]!)
  var accepted = true
  do {
    switch item["kind"] as! String {
    case "host":
      let host = try decoder.decode(SessionHostDescriptionV3.self, from: data)
      guard host.protocolVersion == KernelSessionProtocol.version else { throw NSError(domain:"native-version",code:1) }
      try host.validate()
    case "event": try decoder.decode(SessionEvent.self, from:data).validateEnvelope()
    case "snapshot": try decoder.decode(SessionJournalSnapshot.self, from:data).validate()
    case "page":
      let request = try decoder.decode(SessionHistoryPageRequestV3.self, from: JSONSerialization.data(withJSONObject:item["request"]!))
      try decoder.decode(SessionHistoryPageV3.self, from:data).validate(for:request)
    default: fatalError("unknown fixture kind")
    }
  } catch { accepted = false }
  guard accepted == item["valid"] as! Bool else { fatalError("ClientKit parity mismatch: \(item["name"]!)") }
}
print("CURRENT_CLIENTKIT_PARITY_OK \(cases.count)")
