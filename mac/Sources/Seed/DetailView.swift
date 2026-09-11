import AppKit
import SeedKit
import SwiftUI

struct DetailView: View {
    @Environment(Workspace.self) private var workspace

    var body: some View {
        if let composition = workspace.composition {
            TaskComposer(composition: composition)
                .id(composition.id)
        } else if let task = workspace.selectedTask {
            TaskDetail(task: task)
                .id(task.id)
        } else {
            ContentUnavailableView {
                Label("No Task Selected", systemImage: "square.dashed")
            } description: {
                Text("Choose a task to see its description and history.")
            }
        }
    }
}

/// Nothing is written until the title is committed: `sd` has no delete.
struct TaskComposer: View {
    @Environment(Workspace.self) private var workspace
    let composition: Workspace.Composition

    @State private var title = ""

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            if let parent = composition.parent.flatMap({ workspace.graph[$0] }) {
                HStack(spacing: 5) {
                    Text("New subtask of")
                    StatusIcon(task: parent)
                    Text(parent.title).lineLimit(1)
                }
                .foregroundStyle(.secondary)
                .padding(.bottom, 9)
            }

            WrappingTextField(
                text: $title,
                placeholder: "Name this task",
                font: WrappingTextField.title,
                onCommit: commit,
                onCancel: {
                    // Emptied first: the field is still first responder, and a
                    // final end-editing during teardown would arrive as a commit.
                    title = ""
                    workspace.composition = nil
                },
                takesFocus: true
            )
            .frame(maxWidth: .infinity)

            Text("⏎ to create it · ⎋ to throw it away")
                .foregroundStyle(.tertiary)
                .padding(.top, 9)

            Spacer(minLength: 0)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 20)
    }

    private func commit() {
        workspace.create(title: title, parent: composition.parent)
    }
}

/// A document rather than a form: only the description is read at length.
struct TaskDetail: View {
    @Environment(Workspace.self) private var workspace
    let task: SeedTask

    @State private var title = ""
    @State private var editingTitle = false

    /// Two layouts: reading scrolls the whole pane, editing drops the scroll
    /// view so the editor can be given the height left under the header and
    /// scroll inside itself, which is what keeps the caret on screen.
    var body: some View {
        Group {
            if isEditing {
                editingLayout
            } else {
                readingLayout
            }
        }
        .environment(
            \.openURL,
            OpenURLAction { url in
                guard url.scheme == "seed", let id = Int(url.lastPathComponent) else {
                    return .systemAction
                }
                workspace.reveal(id)
                return .handled
            }
        )
        .onAppear {
            title = task.title
        }
        .onChange(of: task.title) { _, new in
            if !editingTitle { title = new }
        }
        // The only flush a closing window gets.
        .onDisappear {
            commitTitle()
            workspace.commitEditing()
        }
    }

    private var readingLayout: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
                header

                Divider()
                    .padding(.vertical, 18)

                description

                if !task.log.isEmpty {
                    Divider()
                        .padding(.top, 20)
                    activity
                }
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 20)
        }
    }

    /// The activity log is left out while editing; the room it wants is the
    /// room the editor is for.
    private var editingLayout: some View {
        VStack(alignment: .leading, spacing: 0) {
            header

            Divider()
                .padding(.vertical, 18)

            description
        }
        .padding(.horizontal, 24)
        .padding(.top, 20)
        // Less than the top: the footer carries its own hit-target padding.
        .padding(.bottom, 12)
    }

    @ViewBuilder
    private var header: some View {
        WrappingTextField(
            text: $title,
            placeholder: "Untitled task",
            font: WrappingTextField.title,
            onEditingChanged: { editingTitle = $0 },
            onCommit: commitTitle
        )
        .frame(maxWidth: .infinity)

        dates
            .padding(.top, 5)

        controls
            .padding(.top, 16)

        relations
            .padding(.top, 12)
    }

    private var dates: some View {
        HStack(spacing: 5) {
            Button {
                workspace.copyID(task)
            } label: {
                Text("#\(task.id)")
                    .monospacedDigit()
            }
            .buttonStyle(.plain)
            .help("Copy task ID")

            Text("·")
            Text("created \(task.created.formatted(date: .abbreviated, time: .omitted))")
            Text("·")
            Text("edited \(task.modified.formatted(.relative(presentation: .named)))")
        }
        .foregroundStyle(.secondary)
    }

    /// Labels drop to their own row whole; split across two they would read as
    /// two lists.
    private var controls: some View {
        ViewThatFits(in: .horizontal) {
            HStack(spacing: 8) {
                pickers
                LabelsField(task: task)
                Spacer(minLength: 0)
            }

            VStack(alignment: .leading, spacing: 7) {
                HStack(spacing: 8) {
                    pickers
                    Spacer(minLength: 0)
                }
                LabelsField(task: task)
            }
        }
    }

    @ViewBuilder
    private var pickers: some View {
        Picker(selection: statusBinding) {
            ForEach(Status.allCases) { status in
                Label(status.name, systemImage: status.symbol).tag(status)
            }
        } label: {
            EmptyView()
        }
        .labelsHidden()
        .fixedSize()

        Picker(selection: priorityBinding) {
            ForEach(Priority.allCases) { priority in
                Label(priority.name, systemImage: priority.symbol).tag(priority)
            }
        } label: {
            EmptyView()
        }
        .labelsHidden()
        .fixedSize()
    }

    /// A task with neither relation shows neither line; the menus add the first.
    @ViewBuilder
    private var relations: some View {
        let blocks = workspace.graph.blocking(task.id)

        let blockedBy = workspace.graph.blockedBy(task.id)

        if !blockedBy.isEmpty || !blocks.isEmpty {
            VStack(alignment: .leading, spacing: 3) {
                if !blockedBy.isEmpty { line(.blockedBy, blockedBy) }
                if !blocks.isEmpty { line(.blocks, blocks) }
            }
            .foregroundStyle(.secondary)
        }
    }

    private func line(_ relation: Relation, _ tasks: [SeedTask]) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 6) {
            Text(run(relation.name, tasks))

            // Dashed so the one click that edits does not look like the names,
            // which navigate.
            Button {
                workspace.relating = .init(task: task.id, opening: relation)
            } label: {
                Label("Add", systemImage: "plus")
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .padding(.horizontal, 7)
                    .padding(.vertical, 1)
                    .overlay(
                        Capsule().strokeBorder(
                            .quaternary, style: StrokeStyle(lineWidth: 1, dash: [3, 2])
                        )
                    )
            }
            .buttonStyle(.plain)
        }
    }

    /// One attributed run so the names wrap like a sentence.
    private func run(_ name: String, _ tasks: [SeedTask]) -> AttributedString {
        var line = AttributedString("\(name) ")
        for (index, task) in tasks.enumerated() {
            if index > 0 { line += AttributedString(", ") }
            var link = AttributedString(task.title)
            link.link = URL(string: "seed://task/\(task.id)")
            line += link
        }
        return line
    }

    /// Reading is the common case, so rendered text stays selectable with live
    /// links, and editing is a deliberate act. Leaving the editor saves, the
    /// way the title field does; the editor's own undo covers a mistake.
    private var description: some View {
        VStack(alignment: .leading, spacing: 4) {
            if isEditing {
                ScrollingTextView(
                    text: draft,
                    font: ScrollingTextView.body,
                    onCommit: workspace.commitEditing,
                    onCancel: workspace.commitEditing
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .background(.fill.quaternary, in: .rect(cornerRadius: 6))
                // Pulled out by the text inset, so the words keep the rendered
                // description's margin and the background reaches the clip edge.
                .padding(-ScrollingTextView.inset)
                .overlay(alignment: .topLeading) {
                    if draft.wrappedValue.isEmpty {
                        Text("Describe this task")
                            .font(Font(ScrollingTextView.body))
                            .foregroundStyle(.tertiary)
                            // The editor already carries the label.
                            .accessibilityHidden(true)
                            .allowsHitTesting(false)
                    }
                }
            } else if let text = task.description, !text.isEmpty {
                MarkdownView(source: text)
            } else {
                // Empty, so there is nothing to select and the placeholder can
                // be what starts you writing.
                Button(action: workspace.beginEditing) {
                    Text("No description yet.")
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tertiary)
                .pointerStyle(.link)
            }

            // Below the description rather than a tooltip over it, and a button
            // in both states, so starting an edit moves nothing.
            DescriptionFooter(isEditing: isEditing)
        }
    }

    private var activity: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Activity")
                .font(.body.weight(.semibold))
                .foregroundStyle(.secondary)
                .padding(.top, 14)

            ForEach(task.log.reversed(), id: \.self) { entry in
                LogRow(entry: entry)
            }
        }
    }

    private var statusBinding: Binding<Status> {
        Binding(get: { task.status }, set: { workspace.store.edit(task.id, .status($0)) })
    }

    private var priorityBinding: Binding<Priority> {
        Binding(get: { task.priority }, set: { workspace.store.edit(task.id, .priority($0)) })
    }

    private var isEditing: Bool { workspace.editing?.id == task.id }

    private var draft: Binding<String> {
        Binding(get: { workspace.editing?.draft ?? "" }, set: { workspace.editing?.draft = $0 })
    }

    private func commitTitle() {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else {
            title = task.title
            return
        }
        guard trimmed != task.title else { return }
        workspace.store.edit(task.id, .title(trimmed))
    }
}

struct LogRow: View {
    let entry: LogEntry

    var body: some View {
        HStack(alignment: .top, spacing: 9) {
            Circle()
                .fill(.quaternary)
                .frame(width: 6, height: 6)
                .padding(.top, 6)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(entry.agent ?? "Someone")
                        .foregroundStyle(.secondary)
                    Text("·")
                        .foregroundStyle(.tertiary)
                    Text(entry.timestamp, format: .relative(presentation: .named))
                        .foregroundStyle(.tertiary)
                }

                MarkdownView(source: entry.message)
            }
        }
    }
}

/// Its own view so hovering repaints the footer, not the pane. Two buttons
/// rather than one relabelled: pressing Done blurs the editor, which ends the
/// edit before mouse-up, and one button would then fire the reading-state
/// action and reopen it.
private struct DescriptionFooter: View {
    @Environment(Workspace.self) private var workspace
    let isEditing: Bool

    @State private var hovering = false

    var body: some View {
        HStack {
            if isEditing {
                Button(action: workspace.commitEditing) {
                    HStack(alignment: .firstTextBaseline, spacing: 4) {
                        Image(systemName: "checkmark")
                            .imageScale(.small)
                        Text("Done")
                        Text("⎋")
                            .font(.system(size: 12))
                    }
                    .padding(.vertical, 8)
                    .contentShape(.rect)
                }
                .accessibilityLabel("Done")
            } else {
                Button(action: workspace.beginEditing) {
                    // Baseline, not centre: the shortcut is a size down.
                    HStack(alignment: .firstTextBaseline, spacing: 4) {
                        EditPencil()
                            .stroke(style: .init(lineWidth: 1.3, lineCap: .round, lineJoin: .round))
                            .frame(width: 13, height: 13)
                            // A shape has no baseline; this lands its mass on the text's.
                            .alignmentGuide(.firstTextBaseline) { $0[.bottom] - 1.5 }
                        Text("Edit")
                        Text("⌘E")
                            .font(.system(size: 12))
                    }
                    // Stated because a stroked shape is hit-tested on the stroke alone.
                    .padding(.vertical, 8)
                    .contentShape(.rect)
                }
                .accessibilityLabel("Edit")
            }
        }
        .buttonStyle(.plain)
        .foregroundStyle(hovering ? .secondary : .tertiary)
        .pointerStyle(.link)
        .onHover { hovering = $0 }
    }
}

/// SF Symbols' pencils collapse to a diagonal or a smudge at 13pt and this alpha.
private struct EditPencil: Shape {
    func path(in rect: CGRect) -> Path {
        let unit = min(rect.width, rect.height) / 16
        func at(_ x: CGFloat, _ y: CGFloat) -> CGPoint {
            CGPoint(x: rect.minX + x * unit, y: rect.minY + y * unit)
        }

        var path = Path()
        path.move(to: at(11.4, 2.4))
        path.addLine(to: at(13.6, 4.6))
        path.addLine(to: at(5.5, 12.7))
        path.addLine(to: at(2.6, 13.4))
        path.addLine(to: at(3.3, 10.5))
        path.closeSubpath()

        // The ferrule: without it the outline reads as a plain quadrilateral.
        path.move(to: at(10, 3.8))
        path.addLine(to: at(12.2, 6))
        return path
    }
}
