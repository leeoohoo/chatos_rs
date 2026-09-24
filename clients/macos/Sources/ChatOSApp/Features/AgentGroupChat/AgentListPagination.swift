import SwiftUI

struct AgentListPaginationBar: View {
    let totalCount: Int
    @Binding var page: Int
    @Binding var pageSize: Int
    var pageSizeOptions: [Int] = [10, 20, 50]
    var compact = false

    private var pageCount: Int {
        max(1, Int(ceil(Double(totalCount) / Double(max(1, pageSize)))))
    }

    private var rangeLabel: String {
        guard totalCount > 0 else { return "0 项" }
        let start = page * pageSize + 1
        let end = min(totalCount, start + pageSize - 1)
        return "\(start)–\(end) / \(totalCount)"
    }

    var body: some View {
        HStack(spacing: 10) {
            Text(rangeLabel)
                .appFont(.caption2.monospacedDigit())
                .foregroundStyle(.secondary)

            Spacer(minLength: 8)

            if !compact {
                Picker("每页", selection: $pageSize) {
                    ForEach(pageSizeOptions, id: \.self) { size in
                        Text("\(size) / 页").tag(size)
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .controlSize(.small)
            }

            Button {
                page = max(0, page - 1)
            } label: {
                Image(systemName: "chevron.left")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(page == 0)
            .help("上一页")

            if !compact {
                Text("\(Swift.min(page + 1, pageCount)) / \(pageCount)")
                    .appFont(.caption2.monospacedDigit())
                    .frame(minWidth: 42)
            }

            Button {
                page = min(pageCount - 1, page + 1)
            } label: {
                Image(systemName: "chevron.right")
            }
            .buttonStyle(.bordered)
            .controlSize(.small)
            .disabled(page + 1 >= pageCount)
            .help("下一页")
        }
        .onChange(of: totalCount) { _, _ in clampPage() }
        .onChange(of: pageSize) { _, _ in
            page = 0
            clampPage()
        }
        .onAppear { clampPage() }
    }

    private func clampPage() {
        page = min(max(0, page), pageCount - 1)
    }
}

extension RandomAccessCollection {
    func agentPage(index: Int, size: Int) -> [Element] {
        guard !isEmpty, size > 0 else { return [] }
        let safeIndex = Swift.max(0, index)
        let lowerOffset = Swift.min(count, safeIndex * size)
        let upperOffset = Swift.min(count, lowerOffset + size)
        let lower = self.index(startIndex, offsetBy: lowerOffset)
        let upper = self.index(startIndex, offsetBy: upperOffset)
        return Array(self[lower..<upper])
    }
}
