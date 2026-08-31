import ChatOSCore
import SwiftUI

struct MessageTaskGraphCanvas: View {
    let graph: MessageTaskGraphSnapshot
    let selectedTaskID: String?
    let onSelect: (MessageTask, MessageTaskWorkspaceViewModel.InspectorSection?) -> Void
    @State private var zoom = 1.0
    @State private var panOffset: CGSize = .zero
    @State private var previousViewportSize: CGSize = .zero
    @State private var hasPositionedGraph = false
    @State private var magnifyStartZoom: Double?
    @GestureState private var dragTranslation: CGSize = .zero

    var body: some View {
        GeometryReader { proxy in
            let layout = MessageTaskGraphLayout.make(graph: graph)
            let focusContext = MessageTaskGraphFocus.context(
                selectedTaskID: selectedTaskID,
                edges: graph.edges
            )
            ZStack(alignment: .topLeading) {
                DotGridSurface()
                ZStack {
                    graphEdges(layout: layout, focusContext: focusContext)
                    ForEach(graph.nodes) { node in
                        MessageTaskGraphNodeCard(
                            node: node,
                            renderScale: zoom,
                            isSelected: node.id == selectedTaskID,
                            isDimmed: focusContext.map { !$0.relatedTaskIDs.contains(node.id) } ?? false,
                            onSelect: { onSelect(node.task, nil) },
                            onOpenProcess: { onSelect(node.task, .process) },
                            onOpenDetail: { onSelect(node.task, .detail) },
                            onOpenRun: { onSelect(node.task, .run) }
                        )
                        .position(scaled(layout.positions[node.id] ?? .zero))
                    }
                }
                .frame(
                    width: layout.contentSize.width * zoom,
                    height: layout.contentSize.height * zoom
                )
                .offset(
                    x: panOffset.width + dragTranslation.width,
                    y: panOffset.height + dragTranslation.height
                )
            }
            .contentShape(Rectangle())
            .clipped()
            .background(Color(nsColor: .textBackgroundColor))
            .background {
                MessageTaskGraphScrollZoomCapture { delta, location in
                    zoomFromScroll(
                        delta,
                        anchor: location
                    )
                }
                .allowsHitTesting(false)
            }
            .simultaneousGesture(panGesture)
            .simultaneousGesture(magnifyGesture(container: proxy.size))
            .overlay(alignment: .bottomTrailing) {
                zoomControls(container: proxy.size, content: layout.contentSize)
                    .padding(12)
            }
            .onAppear {
                positionGraphIfNeeded(container: proxy.size, layout: layout)
            }
            .onChange(of: proxy.size) {
                handleViewportResize(proxy.size)
            }
            .onChange(of: graphSignature) {
                hasPositionedGraph = false
                positionGraphIfNeeded(container: proxy.size, layout: layout)
            }
        }
    }

    private var panGesture: some Gesture {
        DragGesture(minimumDistance: 3, coordinateSpace: .local)
            .updating($dragTranslation) { value, state, _ in
                state = value.translation
            }
            .onEnded { value in
                panOffset.width += value.translation.width
                panOffset.height += value.translation.height
            }
    }

    private func magnifyGesture(container: CGSize) -> some Gesture {
        MagnificationGesture()
            .onChanged { magnification in
                let startZoom = magnifyStartZoom ?? zoom
                if magnifyStartZoom == nil {
                    magnifyStartZoom = zoom
                }
                setZoom(
                    min(2, max(0.25, startZoom * magnification)),
                    anchor: CGPoint(x: container.width / 2, y: container.height / 2)
                )
            }
            .onEnded { _ in
                magnifyStartZoom = nil
            }
    }

    private var graphSignature: String {
        graph.nodes.map(\.id).sorted().joined(separator: "|")
            + "::"
            + graph.edges.map(\.id).sorted().joined(separator: "|")
    }

    private func graphEdges(
        layout: MessageTaskGraphLayout,
        focusContext: MessageTaskGraphFocusContext?
    ) -> some View {
        Canvas { context, _ in
            for edge in graph.edges {
                guard let source = layout.positions[edge.sourceID],
                      let target = layout.positions[edge.targetID] else { continue }
                let sourcePoint = CGPoint(
                    x: (source.x + MessageTaskGraphLayout.nodeSize.width / 2) * zoom,
                    y: source.y * zoom
                )
                let targetPoint = CGPoint(
                    x: (target.x - MessageTaskGraphLayout.nodeSize.width / 2) * zoom,
                    y: target.y * zoom
                )
                var path = Path()
                path.move(to: sourcePoint)
                let middleX = (sourcePoint.x + targetPoint.x) / 2
                path.addCurve(
                    to: targetPoint,
                    control1: CGPoint(x: middleX, y: sourcePoint.y),
                    control2: CGPoint(x: middleX, y: targetPoint.y)
                )
                let isDirect = focusContext?.directTaskIDs.contains(edge.sourceID) == true
                    && focusContext?.directTaskIDs.contains(edge.targetID) == true
                let isRelated = focusContext.map {
                    $0.relatedTaskIDs.contains(edge.sourceID)
                        && $0.relatedTaskIDs.contains(edge.targetID)
                } ?? true
                let baseColor: Color = edge.kind == "context" ? .secondary : .accentColor
                context.stroke(
                    path,
                    with: .color(baseColor.opacity(isRelated ? 0.82 : 0.20)),
                    style: StrokeStyle(
                        lineWidth: (isDirect ? 2.2 : (isRelated ? 1.7 : 1)) * zoom,
                        dash: edge.kind == "context" ? [6 * zoom, 5 * zoom] : []
                    )
                )
            }
        }
    }

    private func scaled(_ point: CGPoint) -> CGPoint {
        CGPoint(x: point.x * zoom, y: point.y * zoom)
    }

    private func zoomControls(container: CGSize, content: CGSize) -> some View {
        HStack(spacing: 4) {
            Button("缩小", systemImage: "minus") {
                setZoom(max(0.25, zoom - 0.1), container: container)
            }
                .labelStyle(.iconOnly)
            Button("\(Int(zoom * 100))%") {
                setZoom(1, container: container)
            }
                .frame(minWidth: 54)
            Button("放大", systemImage: "plus") {
                setZoom(min(2, zoom + 0.1), container: container)
            }
                .labelStyle(.iconOnly)
            Button("居中", systemImage: "scope") {
                centerGraph(container: container, content: content)
            }
            Button("适应") {
                fitGraph(container: container, content: content)
            }
        }
        .controlSize(.small)
        .padding(7)
        .background(AppPalette.surface, in: RoundedRectangle(cornerRadius: 9))
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(AppPalette.border.opacity(0.8)) }
        .overlay { RoundedRectangle(cornerRadius: 9).stroke(.separator) }
    }

    private func positionGraphIfNeeded(
        container: CGSize,
        layout: MessageTaskGraphLayout
    ) {
        guard !hasPositionedGraph, container.width > 0, container.height > 0 else { return }
        previousViewportSize = container
        zoom = 1
        if let selectedTaskID,
           let selectedPosition = layout.positions[selectedTaskID] {
            centerGraph(
                on: selectedPosition,
                container: container
            )
        } else {
            centerGraph(container: container, content: layout.contentSize)
        }
        hasPositionedGraph = true
    }

    private func handleViewportResize(_ newSize: CGSize) {
        guard newSize.width > 0, newSize.height > 0 else { return }
        guard hasPositionedGraph else {
            return
        }
        panOffset.width += (newSize.width - previousViewportSize.width) / 2
        panOffset.height += (newSize.height - previousViewportSize.height) / 2
        previousViewportSize = newSize
    }

    private func fitGraph(container: CGSize, content: CGSize) {
        zoom = fittedZoom(container: container, content: content)
        centerGraph(container: container, content: content)
    }

    private func fittedZoom(container: CGSize, content: CGSize) -> Double {
        let horizontalPadding: CGFloat = 72
        let verticalPadding: CGFloat = 72
        let widthScale = max(1, container.width - horizontalPadding) / content.width
        let heightScale = max(1, container.height - verticalPadding) / content.height
        return min(1, max(0.25, min(widthScale, heightScale)))
    }

    private func centerGraph(container: CGSize, content: CGSize) {
        panOffset = CGSize(
            width: (container.width - content.width * zoom) / 2,
            height: (container.height - content.height * zoom) / 2
        )
    }

    private func centerGraph(on position: CGPoint, container: CGSize) {
        panOffset = CGSize(
            width: container.width / 2 - position.x * zoom,
            height: container.height / 2 - position.y * zoom
        )
    }

    private func setZoom(_ newZoom: Double, container: CGSize) {
        setZoom(
            newZoom,
            anchor: CGPoint(x: container.width / 2, y: container.height / 2)
        )
    }

    private func setZoom(_ newZoom: Double, anchor: CGPoint) {
        guard zoom > 0, newZoom != zoom else { return }
        let contentCenter = CGPoint(
            x: (anchor.x - panOffset.width) / zoom,
            y: (anchor.y - panOffset.height) / zoom
        )
        zoom = newZoom
        panOffset = CGSize(
            width: anchor.x - contentCenter.x * newZoom,
            height: anchor.y - contentCenter.y * newZoom
        )
    }

    private func zoomFromScroll(_ delta: CGFloat, anchor: CGPoint) {
        guard abs(delta) > 0.01 else { return }
        let factor = min(1.16, max(0.86, exp(Double(delta) * 0.025)))
        setZoom(
            min(2, max(0.25, zoom * factor)),
            anchor: anchor
        )
    }
}
