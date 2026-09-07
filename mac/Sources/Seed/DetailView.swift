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

/// A task does not exist until it is named: `sd` has no delete, so creating one
/// first would leave a dropped task holding an id every time someone changed
/// their mind. Nothing but the title is offered, because nothing else would have
/// anywhere to write.
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
                    // Emptied before the view goes: the field is still first
                    // responder here, and a final `controlTextDidEndEditing`
                    // during teardown would arrive as a commit. An empty title
                    // is refused by `create`, and `sd` has no delete.
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

    /// Nothing typed means nothing happened; anything typed is kept, which is
    /// what the title field beside it already does.
    private func commit() {
        workspace.create(title: title, parent: composition.parent)
    }
}

/// Laid out as a document rather than a form: the description is the only part
/// worth reading at length, so nothing else gets a box or a full-width row.
struct TaskDetail: View {
    @Environment(Workspace.self) private var workspace
    let task: SeedTask

    @State private var title = ""
    @State private var editingTitle = false
    @State private var hoveringDescription = false

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 0) {
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
        .environment(\.openURL, OpenURLAction { url in
            guard url.scheme == "seed", let id = Int(url.lastPathComponent) else {
                return .systemAction
            }
            workspace.reveal(id)
            return .handled
        })
        .onAppear {
            title = task.title
        }
        .onChange(of: task.title) { _, new in
            if !editingTitle { title = new }
        }
        // The only flush a closing window gets; every other path goes through
        // a selection change.
        .onDisappear {
            commitTitle()
            workspace.commitEditing()
        }
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

    /// Labels drop to a row of their own, whole, when they will not fit beside
    /// status and priority — they are one run of text, so splitting them across
    /// two rows would read as two label lists.
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

    /// One wrapping line each, the way labels read: the pane lists things one
    /// way rather than two. A task with neither relation shows neither line —
    /// the menus are how you add the first one.
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

            // Dashed rather than another link: the names navigate, and the one
            // place a click means edit should not look like the ones that don't.
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

    /// Built as one attributed run so the names wrap like a sentence; a stack of
    /// links would break between them wherever the stack decided to.
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

    /// Click to edit: the description renders as markdown until you click into
    /// it, which is the only way it can be both readable and editable in place.
    /// Leaving the field saves, the way the title above it does — there is no
    /// cancel, and the editor's own undo covers a mistake before you leave.
    private var description: some View {
        VStack(alignment: .leading, spacing: 6) {
            if isEditing {
                // The same field as the title, so it grows with its text: a
                // fixed height is a wall of nothing when empty and a scroller
                // inside a scroller when long.
                WrappingTextField(
                    text: draft,
                    placeholder: "Describe this task",
                    font: .monospacedSystemFont(
                        ofSize: NSFont.preferredFont(forTextStyle: .body).pointSize,
                        weight: .regular
                    ),
                    onCommit: workspace.commitEditing,
                    onCancel: workspace.commitEditing,
                    takesFocus: true,
                    insertsNewlines: true,
                    selectsOnFocus: false
                )
                .frame(maxWidth: .infinity)
            } else {
                rendered
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(6)
                    .background(
                        hoveringDescription ? Color.primary.opacity(0.04) : .clear,
                        in: .rect(cornerRadius: 6)
                    )
                    .padding(-6)
                    .contentShape(.rect)
                    .onHover { hoveringDescription = $0 }
                    .onTapGesture(perform: beginEditing)
            }

            // Below the description rather than over it, and always taking its
            // own height: a tooltip covers the words it is describing, and a
            // line that comes and goes moves the text underneath it.
            Text(isEditing ? "⌘E, ⎋, or click away to save" : "⌘E or click to edit")
                .foregroundStyle(.tertiary)
                .opacity(isEditing || hoveringDescription ? 1 : 0)
        }
    }

    @ViewBuilder
    private var rendered: some View {
        if let text = task.description, !text.isEmpty {
            MarkdownView(source: text)
        } else {
            Text("No description yet.")
                .foregroundStyle(.tertiary)
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
        Binding(get: { task.status }) { new in
            workspace.edit(task.id, .status(new))
        }
    }

    private var priorityBinding: Binding<Priority> {
        Binding(get: { task.priority }) { new in
            workspace.edit(task.id, .priority(new))
        }
    }

    private var isEditing: Bool { workspace.editing?.id == task.id }

    private var draft: Binding<String> {
        Binding(get: { workspace.editing?.draft ?? "" }) { workspace.editing?.draft = $0 }
    }

    private func beginEditing() {
        workspace.beginEditing()
    }

    private func commitTitle() {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return title = task.title }
        guard trimmed != task.title else { return }
        workspace.edit(task.id, .title(trimmed))
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
                    .textSelection(.enabled)
            }
        }
    }
}
