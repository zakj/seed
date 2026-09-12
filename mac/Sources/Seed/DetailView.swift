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

    /// Reading is a document and editing is a form, and they are different
    /// layouts rather than one layout with a field swapped in. A scroll view
    /// offers no height along the axis it scrolls, so nothing inside one can
    /// fill the space that is left — and an editor that cannot be given the
    /// space that is left has to size itself to its text, which puts the end
    /// of a long description past the bottom of the window with nothing able
    /// to scroll to it. Editing drops the scroll view for that reason: the
    /// header and the footer take what they need, the editor takes the rest
    /// and scrolls inside itself, and the caret is AppKit's to keep in view.
    var body: some View {
        Group {
            if isEditing {
                editingLayout
            } else {
                readingLayout
            }
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

    /// The activity log is not shown while editing: it is the one part of the
    /// pane that is neither the thing being edited nor the context for it, and
    /// the room it wants is the room the editor is for.
    private var editingLayout: some View {
        VStack(alignment: .leading, spacing: 0) {
            header

            Divider()
                .padding(.vertical, 18)

            // The block is offered the rest of the pane and the editor takes
            // what its text needs of it, so a long description fills to the
            // footer and a short one sits under the meta lines where it was
            // read, with the leftover below them both.
            description
                .frame(maxHeight: .infinity, alignment: .top)
        }
        .padding(.horizontal, 24)
        .padding(.top, 20)
        // Less than the top: the footer carries its own hit-target padding,
        // and the two together read as a bigger gap than the one above.
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

    /// Reading is the common case — agents write most descriptions, people read
    /// all of them — so rendered text stays selectable and its links stay live,
    /// and editing is a deliberate act rather than a click anywhere. Leaving the
    /// field saves, the way the title above it does — there is no cancel, and the
    /// editor's own undo covers a mistake before you leave.
    private var description: some View {
        VStack(alignment: .leading, spacing: 4) {
            if isEditing {
                // Grows to its text and stops at the room the layout has
                // left, with two lines as its floor: empty it reads as
                // somewhere to write rather than a wall of nothing, and long
                // it scrolls inside itself rather than running off the bottom
                // of the window.
                ScrollingTextView(
                    text: draft,
                    font: ScrollingTextView.body,
                    onCommit: workspace.commitEditing,
                    onCancel: workspace.commitEditing
                )
                .frame(maxWidth: .infinity)
                // The editor is a region with an extent, and while editing it
                // has to look like one: text that stops against nothing reads
                // as clipped by accident. Padded outward so the words stay on
                // the same left margin as the rendered description they
                // replace, and so the last line runs under an edge rather
                // than off one.
                .background(
                    .fill.quaternary, in: RoundedRectangle(cornerRadius: 6).inset(by: -8)
                )
                .overlay(alignment: .topLeading) {
                    if draft.wrappedValue.isEmpty {
                        Text("Describe this task")
                            .font(Font(ScrollingTextView.body))
                            .foregroundStyle(.tertiary)
                            // The editor behind it carries the label; this is
                            // the same words a second time to a reader who
                            // cannot see that it is a watermark.
                            .accessibilityHidden(true)
                            .allowsHitTesting(false)
                    }
                }
            } else if let text = task.description, !text.isEmpty {
                MarkdownView(source: text)
            } else {
                // Empty, so there is no text to select and no link to swallow:
                // the placeholder can still be what starts you writing.
                Button(action: workspace.beginEditing) {
                    Text("No description yet.")
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(.rect)
                }
                .buttonStyle(.plain)
                .foregroundStyle(.tertiary)
                .pointerStyle(.link)
            }

            // Below the description rather than over it, and always taking its
            // own height: a tooltip covers the words it is describing, and a line
            // that comes and goes moves the text underneath it. The same control
            // in both states, so the slot never turns from a button into prose.
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
            }
        }
    }
}

/// Its own view so hovering repaints the footer rather than the pane around it:
/// a `TaskDetail` body pass walks the graph for both relation lists and rebuilds
/// their attributed strings.
///
/// Two buttons rather than one whose label changes: pressing Done blurs the
/// editor, which commits and ends the edit before the mouse comes up, and one
/// button would keep its identity across that, complete the press, and fire
/// the reading-state action — reopening the editor it just closed.
///
/// Reading and editing are separate layouts, so this whole view is rebuilt on
/// the way between them either way; the two branches are what make that
/// harmless rather than what depends on it.
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
                    // Baseline, not centre: the shortcut is a size down, and
                    // centring floats it above the word it belongs to.
                    HStack(alignment: .firstTextBaseline, spacing: 4) {
                        EditPencil()
                            .stroke(style: .init(lineWidth: 1.3, lineCap: .round, lineJoin: .round))
                            .frame(width: 13, height: 13)
                            // A shape has no baseline of its own, and its bottom
                            // edge sits the drawing low; this lands its mass on
                            // the text's baseline instead.
                            .alignmentGuide(.firstTextBaseline) { $0[.bottom] - 1.5 }
                        Text("Edit")
                        Text("⌘E")
                            .font(.system(size: 12))
                    }
                    // A stroked shape is hit-tested on the stroke itself, and the
                    // gaps between the three pieces are not hit-tested at all, so
                    // the target has to be stated. The padding is real, not padded
                    // back off: hit-testing is clipped to the frame, so a target
                    // taller than the text costs the space it occupies.
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

/// SF Symbols' `pencil` collapses to a bare diagonal at this size and alpha, and
/// `square.and.pencil` shrunk to match the text reads as a smudge rather than an
/// icon. Drawn as an outline, the silhouette carries the shape at 13pt.
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
