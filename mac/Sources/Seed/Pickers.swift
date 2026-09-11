import AppKit
import SeedKit
import SwiftUI

// Both pickers are filter-first: focus stays in the filter field, which steps
// the highlight itself, so the list below never needs focus. See design.md for
// why a `List` cannot do this.

/// One sheet for all three relations; the segments switch without losing the
/// filter or the highlight. A sheet because the menu bar opens it too.
struct TaskPicker: View {
    @Environment(Workspace.self) private var workspace
    @Environment(\.dismiss) private var dismiss
    let relating: Workspace.Relating

    @State private var filter = ""
    @State private var highlight: Int?
    @State private var relation: Relation

    init(relating: Workspace.Relating) {
        self.relating = relating
        _relation = State(initialValue: relating.opening)
    }

    var body: some View {
        // Built once per pass; the sheet re-renders on every keystroke.
        let candidates = candidates
        let matches = matches(in: candidates)
        let ids = matches.map(\.task.id)

        return VStack(alignment: .leading, spacing: 0) {
            // Headed with the task; the segments name the relation.
            Text(workspace.graph[relating.task]?.title ?? "Task #\(relating.task)")
                .font(.headline)
                .lineLimit(1)
                .padding(.horizontal, 14)
                .padding(.top, 14)

            Picker("Relation", selection: $relation) {
                ForEach(Relation.allCases) { relation in
                    Text(relation.name).tag(relation)
                }
            }
            .pickerStyle(.segmented)
            .labelsHidden()
            .padding(.horizontal, 14)
            .padding(.top, 10)

            TextField("Filter, or #id", text: $filter)
                .textFieldStyle(.roundedBorder)
                .padding(14)
                .highlightKeys(over: ids, highlight: $highlight, activate: relate)
                // ⌘⇧[ and ⌘⇧]: a segmented control is not in the tab order.
                .onKeyPress(phases: .down, action: switchRelation)

            Divider()

            ScrollViewReader { list in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(matches) { candidate in
                            row(candidate)
                        }
                    }
                    .padding(.vertical, 4)
                }
                .scrollsToHighlight(list, over: ids, highlight: highlight)
            }
            .frame(height: 260)
            .overlay {
                if matches.isEmpty { Text(emptiness).foregroundStyle(.secondary) }
            }
            .safeAreaInset(edge: .bottom, spacing: 0) {
                if let reason = barrier(in: candidates) {
                    Text(reason)
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .padding(.horizontal, 14)
                        .padding(.vertical, 7)
                        .background(.fill.quaternary)
                }
            }

            Divider()

            HStack {
                Spacer()
                // Return belongs to the filter field, so Escape closes the sheet.
                Button("Done") { dismiss() }
                    .keyboardShortcut(.cancelAction)
            }
            .padding(14)
        }
        .frame(width: 360)
    }

    private func switchRelation(_ press: KeyPress) -> KeyPress.Result {
        guard press.modifiers.contains([.command, .shift]) else { return .ignored }
        let step: Int
        switch press.key {
        case "[", "{": step = -1
        case "]", "}": step = 1
        default: return .ignored
        }
        let all = Relation.allCases
        guard let index = all.firstIndex(of: relation) else { return .ignored }
        relation = all[(index + step + all.count) % all.count]
        return .handled
    }

    private var emptiness: String {
        workspace.graph.emptiness(for: relating.task, by: relation)
    }

    private func row(_ candidate: Candidate) -> some View {
        let task = candidate.task

        return PickerRow(ticked: isRelated(task.id), highlighted: highlight == task.id) {
            highlight = task.id
            relate(task.id)
        } content: {
            StatusIcon(task: task)
            Text(task.title)
                .lineLimit(1)
            Spacer(minLength: 8)
            Text(String(task.id))
                .monospacedDigit()
                .foregroundStyle(.tertiary)
        }
        // Dimmed rather than hidden, so an arrow landing here still reads as a move.
        .opacity(candidate.barred == nil ? 1 : 0.35)
        .disabled(candidate.barred != nil)
        .help(candidate.barred ?? "")
        .id(task.id)
    }

    /// `Store.relate` is what refuses a barred candidate; this does not re-check.
    private func relate(_ id: Int) {
        workspace.store.relate(id, to: relating.task, by: relation, on: !isRelated(id))
    }

    /// `help` is hover-only; a keyboard user needs the reason on screen.
    private func barrier(in candidates: [Candidate]) -> String? {
        highlight.flatMap { id in candidates.first { $0.task.id == id }?.barred }
    }

    private func isRelated(_ id: Int) -> Bool {
        workspace.graph.isRelated(id, to: relating.task, by: relation)
    }

    private var candidates: [Candidate] {
        workspace.graph.candidates(for: relating.task, by: relation)
    }

    private func matches(in candidates: [Candidate]) -> [Candidate] {
        let query = filter.trimmingCharacters(in: .whitespaces)
        guard !query.isEmpty else { return candidates }
        return candidates.filter { $0.task.matches(query) }
    }
}

/// The row both pickers draw: a tick, content, a highlight.
private struct PickerRow<Content: View>: View {
    let ticked: Bool
    let highlighted: Bool
    let action: () -> Void
    @ViewBuilder let content: Content

    var body: some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: "checkmark")
                    .opacity(ticked ? 1 : 0)
                    .frame(width: 12)
                content
            }
            .padding(.horizontal, 6)
            .padding(.vertical, 3)
            .contentShape(.rect)
            .highlighted(highlighted)
            .padding(.horizontal, 4)
        }
        .buttonStyle(.plain)
    }
}

extension View {
    /// Keeps the highlighted row on screen when the highlight moves or the rows
    /// under it do.
    func scrollsToHighlight<ID: Hashable>(
        _ list: ScrollViewProxy, over ids: [ID], highlight: ID?
    ) -> some View {
        onChange(of: highlight) { _, id in
            if let id { list.scrollTo(id) }
        }
        .onChange(of: ids) { _, _ in
            if let highlight { list.scrollTo(highlight) }
        }
    }
}

/// The filter field steps the highlight; the list below never sees an arrow key.
private struct HighlightKeys<ID: Hashable>: ViewModifier {
    let ids: [ID]
    @Binding var highlight: ID?
    let activate: (ID) -> Void

    func body(content: Content) -> some View {
        content
            .onKeyPress(.upArrow) { step(-1) }
            .onKeyPress(.downArrow) { step(1) }
            .onSubmit {
                if let highlight { activate(highlight) }
            }
            // A narrowed filter keeps a surviving highlight, else takes the first row.
            .onChange(of: ids, initial: true) { _, ids in
                guard highlight.map(ids.contains) != true else { return }
                highlight = ids.first
            }
    }

    private func step(_ delta: Int) -> KeyPress.Result {
        guard !ids.isEmpty else { return .ignored }
        let index = highlight.flatMap(ids.firstIndex(of:)) ?? (delta > 0 ? -1 : ids.count)
        highlight = ids[min(max(index + delta, 0), ids.count - 1)]
        return .handled
    }
}

extension View {
    func highlightKeys<ID: Hashable>(
        over ids: [ID], highlight: Binding<ID?>, activate: @escaping (ID) -> Void
    ) -> some View {
        modifier(HighlightKeys(ids: ids, highlight: highlight, activate: activate))
    }

    func highlighted(_ on: Bool) -> some View {
        background(
            on ? AnyShapeStyle(.selection) : AnyShapeStyle(.clear), in: .rect(cornerRadius: 5))
    }
}

/// Labels read as text: glanced at far more often than changed.
struct LabelsField: View {
    @Environment(Workspace.self) private var workspace
    let task: SeedTask

    @State private var filter = ""
    @State private var highlight: Entry?

    private var picking: Binding<Bool> {
        Binding(
            get: { workspace.labelling == task.id },
            set: { workspace.labelling = $0 ? task.id : nil })
    }

    var body: some View {
        Button {
            workspace.labelling = task.id
        } label: {
            Label(
                task.labels.isEmpty ? "Labels" : task.labels.joined(separator: ", "),
                systemImage: "tag"
            )
            .lineLimit(1)
            .foregroundStyle(task.labels.isEmpty ? .tertiary : .secondary)
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .help("Edit labels")
        .popover(isPresented: picking, arrowEdge: .bottom) { picker }
        // Cleared as the popover opens rather than inside it: the menu bar opens
        // it too, and the popover claims its highlight before `onAppear`.
        .onChange(of: workspace.labelling) { _, id in
            guard id == task.id else { return }
            filter = ""
            highlight = nil
        }
    }

    /// Fixed size: a popover takes its size at presentation and never resizes,
    /// and a menu would take its size from the repository rather than the task.
    private var picker: some View {
        // Built once per pass; the body re-runs on every keystroke.
        let entries = entries

        return VStack(spacing: 0) {
            TextField("Filter or create", text: $filter)
                .textFieldStyle(.roundedBorder)
                .padding(8)
                .highlightKeys(over: entries, highlight: $highlight, activate: activate)

            Divider()

            ScrollViewReader { list in
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 0) {
                        ForEach(entries, id: \.self) { entry in
                            row(entry)
                        }
                    }
                    .padding(.vertical, 4)
                }
                .scrollsToHighlight(list, over: entries, highlight: highlight)
            }
            .frame(height: 200)
        }
        .frame(width: 220)
    }

    private func row(_ entry: Entry) -> some View {
        PickerRow(ticked: ticked(entry), highlighted: highlight == entry) {
            highlight = entry
            activate(entry)
        } content: {
            Text(name(entry))
                .lineLimit(1)
            Spacer(minLength: 0)
        }
        .id(entry)
    }

    /// Creating sits in the list rather than after it, so ↓ reaches it.
    private enum Entry: Hashable {
        case label(String)
        /// Carries no name: a payload changing per keystroke would drop a
        /// highlight resting here.
        case create
    }

    private var entries: [Entry] {
        matches.map(Entry.label) + (creatable ? [.create] : [])
    }

    private func name(_ entry: Entry) -> String {
        switch entry {
        case .label(let label): label
        case .create: "Create “\(trimmedFilter)”"
        }
    }

    private func ticked(_ entry: Entry) -> Bool {
        guard case .label(let label) = entry else { return false }
        return task.labels.contains(label)
    }

    private func activate(_ entry: Entry) {
        switch entry {
        case .label(let label):
            workspace.store.edit(
                task.id, task.labels.contains(label) ? .removeLabel(label) : .addLabel(label))
        case .create:
            // The filter stays: clearing it widens `entries` before the reload
            // lands and moves the highlight.
            workspace.store.edit(task.id, .addLabel(trimmedFilter))
        }
    }

    private var trimmedFilter: String {
        filter.trimmingCharacters(in: .whitespaces)
    }

    private var matches: [String] {
        guard !trimmedFilter.isEmpty else { return workspace.labels }
        return workspace.labels.filter { $0.localizedStandardContains(trimmedFilter) }
    }

    private var creatable: Bool {
        !trimmedFilter.isEmpty && !workspace.labels.contains(trimmedFilter)
    }
}
