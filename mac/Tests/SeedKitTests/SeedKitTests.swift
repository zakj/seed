import Foundation
import Testing
@testable import SeedKit

private func decode(_ json: String) throws -> [SeedTask] {
    try JSONDecoder().decode([SeedTask].self, from: Data(json.utf8))
}

@Test func decodesOmittedFieldsAsDefaults() throws {
    let tasks = try decode("""
    [{"id":9,"title":"Two-way sync","status":"todo",
      "created":"2026-03-05T20:34:34.158526Z","modified":"2026-03-13T02:20:30.840129Z"}]
    """)
    let task = try #require(tasks.first)
    #expect(task.priority == .normal)
    #expect(task.labels.isEmpty)
    #expect(task.parent == nil)
    #expect(task.log.isEmpty)
    #expect(task.depends.isEmpty)
    // Gates the count on an archive menu item that runs behind a confirmation
    // with no undo, so a wrong default here mis-states what is about to move.
    #expect(task.archived == false)
}

@Test func parsesMicrosecondTimestamps() throws {
    let task = try #require(try decode("""
    [{"id":1,"title":"t","status":"done",
      "created":"2026-03-05T20:34:21.577676Z","modified":"2026-03-05T20:34:21Z"}]
    """).first)
    #expect(abs(task.created.timeIntervalSince1970 - 1_772_742_861.577676) < 1e-6)
    #expect(task.modified.timeIntervalSince1970 == 1_772_742_861)
}

private func task(
    _ id: Int, status: Status = .todo, priority: Priority = .normal,
    parent: Int? = nil, depends: [Int] = [], children: [Int] = []
) throws -> SeedTask {
    let fields: [String] = [
        #""id":\#(id)"#, #""title":"t\#(id)""#, #""status":"\#(status.rawValue)""#,
        #""priority":"\#(priority.rawValue)""#, #""depends":\#(depends)"#,
        #""children":\#(children)"#, parent.map { #""parent":\#($0)"# },
        #""created":"2026-01-01T00:00:00Z""#, #""modified":"2026-01-01T00:00:00Z""#,
    ].compactMap { $0 }
    return try #require(try decode("[{\(fields.joined(separator: ","))}]").first)
}

@Test func ordersSiblingsLikeTheCLI() throws {
    let graph = TaskGraph([
        try task(1, status: .done),
        try task(2, status: .todo, depends: [1]),
        try task(3, status: .todo, priority: .low),
        try task(4, status: .todo),
        try task(5, status: .inProgress),
    ])
    #expect(graph.outline(expanded: []).map(\.id) == [5, 4, 3, 2, 1])
}

@Test func nestsChildrenUnderParents() throws {
    let graph = TaskGraph([
        try task(1, children: [3, 2]),
        try task(2, parent: 1),
        try task(3, priority: .high, parent: 1),
    ])
    let rows = graph.outline(expanded: [1])
    #expect(rows.map(\.id) == [1, 3, 2])
    #expect(rows.map(\.depth) == [0, 1, 1])
    #expect(rows.map(\.hasChildren) == [true, false, false])
}

@Test func closedParentHidesItsChildren() throws {
    let graph = TaskGraph([
        try task(1, children: [2]),
        try task(2, parent: 1, children: [3]),
        try task(3, parent: 2),
    ])
    #expect(graph.outline(expanded: []).map(\.id) == [1])
    #expect(graph.outline(expanded: [1]).map(\.id) == [1, 2])
    #expect(graph.outline(expanded: [1, 2]).map(\.id) == [1, 2, 3])
}

@Test func searchKeepsTheTasksAboveAMatch() throws {
    let graph = TaskGraph([
        try task(1, children: [2]),
        try task(2, parent: 1, children: [3]),
        try task(3, parent: 2),
        try task(4),
    ])
    let rows = graph.outline { $0.id == 3 }
    #expect(rows.map(\.id) == [1, 2, 3])
    #expect(rows.map(\.matches) == [false, false, true])
    #expect(graph.outline { $0.id == 99 }.isEmpty)
}

@Test func blockingIsTheReverseOfDepends() throws {
    let graph = TaskGraph([
        try task(1),
        try task(2, depends: [1]),
        try task(3, depends: [1]),
    ])
    #expect(graph.blocking(1).map(\.id) == [2, 3])
    #expect(graph.blocking(2).isEmpty)
}

/// Offering one of these would build a loop `sd` then refuses.
@Test func dependentsGatherTheWholeChain() throws {
    let graph = TaskGraph([
        try task(1),
        try task(2, depends: [1]),
        try task(3, depends: [2]),
    ])
    #expect(graph.dependents(of: 1) == [2, 3])
    #expect(graph.dependents(of: 3).isEmpty)
}

@Test func dependenciesGatherTheWholeChain() throws {
    let graph = TaskGraph([
        try task(1),
        try task(2, depends: [1]),
        try task(3, depends: [2]),
    ])
    #expect(graph.dependencies(of: 3) == [1, 2])
    #expect(graph.dependencies(of: 1).isEmpty)
}

/// Task files are hand-editable, so a parent loop has to end the walk.
@Test func ancestorsStopOnALoop() throws {
    let graph = TaskGraph([
        try task(1, parent: 2),
        try task(2, parent: 1),
    ])
    #expect(graph.ancestors(of: 1) == [2])
}

@Test func descendantsGatherTheWholeBranch() throws {
    let graph = TaskGraph([
        try task(1, children: [2]),
        try task(2, parent: 1, children: [3]),
        try task(3, parent: 2),
    ])
    #expect(graph.descendants(of: 1) == [2, 3])
    #expect(graph.descendants(of: 3).isEmpty)
}

/// What the app opens to put a task the user did not click on screen.
@Test func ancestorsRunFromTheTaskToItsRoot() throws {
    let graph = TaskGraph([
        try task(1, children: [2]),
        try task(2, parent: 1, children: [3]),
        try task(3, parent: 2),
    ])
    #expect(graph.ancestors(of: 3) == [2, 1])
    #expect(graph.ancestors(of: 1).isEmpty)
}

@Test func orphanedChildBecomesRoot() throws {
    let graph = TaskGraph([try task(7, parent: 99)])
    #expect(graph.outline(expanded: []).map(\.id) == [7])
}

@Test func nextExcludesBlockedAndUnfinishedParents() throws {
    let graph = TaskGraph([
        try task(1, children: [2]),
        try task(2, parent: 1),
        try task(3, depends: [2]),
        try task(4),
    ])
    #expect(graph.tasks.filter(graph.isNext).map(\.id) == [2, 4])
}

private func spans(
    _ parts: (String, InlinePresentationIntent?)...
) -> AttributedString {
    parts.reduce(into: AttributedString()) { result, part in
        var piece = AttributedString(part.0)
        piece.inlinePresentationIntent = part.1
        result += piece
    }
}

@Test func parsesBlockStructure() {
    let blocks = Markdown.parse("""
    # Title

    Some **bold** text
    wrapped over lines.

    - one
      - nested
    1. first

    > quoted
    > more

    ```rust
    fn main() {}
    ```

    ---

    | A | B |
    |---|---|
    | 1 | 2 |
    """)
    #expect(blocks == [
        .heading(level: 1, text: "Title"),
        .paragraph(spans(("Some ", nil), ("bold", .stronglyEmphasized), (" text wrapped over lines.", nil))),
        .listItem(indent: 0, marker: .bullet, checked: nil, text: "one"),
        .listItem(indent: 1, marker: .bullet, checked: nil, text: "nested"),
        .listItem(indent: 0, marker: .ordered(1), checked: nil, text: "first"),
        .quote([.paragraph("quoted more")]),
        .code(language: "rust", text: "fn main() {}"),
        .rule,
        .table(header: ["A", "B"], rows: [["1", "2"]]),
    ])
}

@Test func parsesTaskListCheckboxes() {
    #expect(Markdown.parse("- [ ] open\n- [x] closed") == [
        .listItem(indent: 0, marker: .bullet, checked: false, text: "open"),
        .listItem(indent: 0, marker: .bullet, checked: true, text: "closed"),
    ])
}

@Test func keepsBlockStructureInsideQuotes() {
    #expect(Markdown.parse("> intro\n>\n> ```\n> fn main() {}\n> ```") == [
        .quote([
            .paragraph("intro"),
            .code(language: nil, text: "fn main() {}"),
        ])
    ])
}

@Test func hardBreakSurvivesButSoftBreakBecomesSpace() {
    #expect(Markdown.parse("line one  \nline two") == [.paragraph("line one\nline two")])
    #expect(Markdown.parse("line one\nline two") == [.paragraph("line one line two")])
}

@Test func codeFenceKeepsRelativeIndentation() {
    #expect(Markdown.parse("""
    ```
      a
        b
    ```
    """) == [.code(language: nil, text: "  a\n    b")])
}

@Test func unclosedFenceRunsToEnd() {
    #expect(Markdown.parse("```\nx") == [.code(language: nil, text: "x")])
}

@Test func keepsValuesStartingWithADashOutOfFlagPosition() {
    #expect(Edit.description("- first\n- second").arguments == ["--description=- first\n- second"])
    #expect(Edit.title("-dashy").arguments == ["--title=-dashy"])
    #expect(Edit.addLabel("-x").arguments == ["--add-label=-x"])
}

@Test func onlyMarkingDoneOffersForce() {
    #expect(Edit.status(.done).forceable)
    #expect(!Edit.status(.dropped).forceable)
    #expect(!Edit.title("done").forceable)
}

@Test func readsErrorsOutOfTheJSONEnvelope() {
    let json = Data(#"{"error":"KDL parse error: Failed to parse KDL document"}"#.utf8)
    #expect(SeedCLI.failureMessage(json) == "KDL parse error: Failed to parse KDL document")

    let plain = Data("error: KDL parse error\n".utf8)
    #expect(SeedCLI.failureMessage(plain) == "error: KDL parse error")
}

@Test func escapedPunctuationSurvivesToTheRenderer() {
    #expect(Markdown.parse(#"a \*literal\* b"#) == [.paragraph("a *literal* b")])
    #expect(Markdown.parse(#"inline \[x\](y)"#) == [.paragraph("inline [x](y)")])
    #expect(Markdown.parse(#"snake\_case\_name"#) == [.paragraph("snake_case_name")])
}

@Test func nestedEmphasisKeepsBothIntents() {
    let blocks = Markdown.parse("**bold *and italic***")
    #expect(blocks == [.paragraph(spans(
        ("bold ", .stronglyEmphasized),
        ("and italic", [.stronglyEmphasized, .emphasized])
    ))])
}

// MARK: - Recents

/// In memory rather than a scratch `UserDefaults` suite: a suite cannot be
/// cleaned up from inside the test process — cfprefsd writes the domain back
/// out after exit, leaving an empty plist in ~/Library/Preferences however
/// thoroughly the test removes it on the way out.
private final class MemoryStore: RecentsStore {
    private var values: [String: Any] = [:]

    func stringArray(forKey key: String) -> [String]? { values[key] as? [String] }
    func set(_ value: Any?, forKey key: String) { values[key] = value }
}

private func withScratchDefaults(_ body: (MemoryStore) throws -> Void) throws {
    try body(MemoryStore())
}

/// `Recents` drops entries that are no longer repositories, so these have to be
/// real directories carrying a real `.seed`.
private let recentsRoot: URL = {
    let root = URL(filePath: NSTemporaryDirectory())
        .appending(path: "seed-recents-\(UUID().uuidString)")
    try? FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
    atexit_b { try? FileManager.default.removeItem(at: root) }
    return root
}()

private func repository(_ name: String) -> URL {
    let url = SeedCLI.directory(recentsRoot.appending(path: name))
    try? FileManager.default.createDirectory(
        at: url.appending(path: ".seed"), withIntermediateDirectories: true)
    return url
}

@MainActor
@Test func recentsLeadWithTheLastOpenedAndNeverRepeat() throws {
    try withScratchDefaults { defaults in
        let recents = Recents(defaults: defaults)
        for name in ["one", "two", "three"] { recents.add(repository(name)) }
        #expect(recents.urls.map(\.lastPathComponent) == ["three", "two", "one"])

        recents.add(repository("one"))
        #expect(recents.urls.map(\.lastPathComponent) == ["one", "three", "two"])
    }
}

@MainActor
@Test func recentsStopAtTen() throws {
    try withScratchDefaults { defaults in
        let recents = Recents(defaults: defaults)
        for index in 1...14 { recents.add(repository("r\(index)")) }
        #expect(recents.urls.count == 10)
        #expect(recents.urls.first?.lastPathComponent == "r14")
        #expect(recents.urls.last?.lastPathComponent == "r5")
    }
}

@MainActor
@Test func recentsSurviveAndClear() throws {
    try withScratchDefaults { defaults in
        let recents = Recents(defaults: defaults)
        recents.add(repository("kept"))
        #expect(Recents(defaults: defaults).urls == recents.urls)

        recents.clear()
        #expect(Recents(defaults: defaults).urls.isEmpty)
    }
}

@MainActor
@Test func recentsSkipAHeadThatIsGone() throws {
    try withScratchDefaults { defaults in
        let kept = repository("kept")
        let removed = repository("removed")
        let recents = Recents(defaults: defaults)
        recents.add(kept)
        recents.add(removed)

        try FileManager.default.removeItem(at: removed)
        // The menu still lists it — clicking a dead entry deliberately is a
        // different thing from a new window silently landing on one.
        #expect(recents.urls.count == 2)
        #expect(recents.firstWorkspace == kept)
    }
}

/// `sd` stands in as any command: what these cover is `run` itself — the pipes,
/// the exit code, the cancellation — none of which is about `sd` in particular.
private func sh(_ script: String) async throws -> Data {
    try await SeedCLI.run(
        ["-c", script],
        binary: URL(filePath: "/bin/sh"),
        repository: SeedCLI.directory(URL(filePath: "/tmp"))
    )
}

/// Both pipes are drained at once because draining one to its end *before* the
/// other deadlocks as soon as the second fills its buffer. Verified: replacing
/// the two `async let`s with sequential `readDataToEndOfFile` calls hangs here
/// and passes every other test in this file.
@Test(.timeLimit(.minutes(1))) func drainsBothPipesPastTheBufferLimit() async throws {
    let data = try await sh("yes onlyout | head -c 2000000; yes onlyerr | head -c 2000000 1>&2")
    #expect(data.count == 2_000_000)
}

/// Commands run one at a time, so a cancelled read that keeps its subprocess
/// alive holds up every write queued behind it.
@Test(.timeLimit(.minutes(1))) func cancellingStopsTheSubprocess() async throws {
    let started = Date()
    let task = Task { try await sh("sleep 30") }
    try await Task.sleep(for: .milliseconds(200))
    task.cancel()
    _ = try? await task.value
    #expect(Date().timeIntervalSince(started) < 5)
}

/// A nonzero exit carries the CLI's own message; `sd` explains a refused edit
/// on stderr and the alert shows exactly that text.
@Test(.timeLimit(.minutes(1))) func surfacesStderrFromAFailedCommand() async throws {
    let error = await #expect(throws: (any Error).self) {
        try await sh("echo 'unmet dependencies: #1 not done' >&2; exit 1")
    }
    #expect(error?.localizedDescription.contains("unmet dependencies: #1 not done") == true)
}

/// A failure with nothing on stderr still has to say something an alert can show.
@Test(.timeLimit(.minutes(1))) func fallsBackToTheStatusWhenStderrIsSilent() async throws {
    let error = await #expect(throws: (any Error).self) { try await sh("exit 3") }
    #expect(error?.localizedDescription.contains("3") == true)
}

/// A command that cannot even start has to report that, not hang: commands run
/// one at a time, so one that never returns takes every later write with it.
@Test(.timeLimit(.minutes(1))) func reportsAProcessThatCannotStart() async throws {
    let missing = URL(filePath: "/private/tmp/seed-not-here-\(UUID().uuidString)", directoryHint: .isDirectory)

    await #expect(throws: (any Error).self) {
        try await SeedCLI.run(["list"], binary: URL(filePath: "/bin/echo"), repository: missing)
    }
}

@Test func keepsATitleStartingWithADashOutOfFlagPosition() {
    #expect(SeedCLI.addArguments(title: "-n is not a flag here", parent: nil)
        == ["add", "--quiet", "--", "-n is not a flag here"])
    // Options have to land before the separator or `sd` reads them as title.
    #expect(SeedCLI.addArguments(title: "child", parent: 7)
        == ["add", "--quiet", "--parent=7", "--", "child"])
}

@Test func decodesALogEntry() throws {
    // One bad entry fails the whole `[SeedTask]` decode, which takes the task
    // list down with it.
    let task = try #require(try decode("""
    [{"id":1,"title":"t","status":"todo",
      "created":"2026-03-05T20:34:21Z","modified":"2026-03-05T20:34:21Z",
      "log":[{"timestamp":"2026-03-05T20:34:21.5Z","message":"did a thing","agent":"claude"},
             {"timestamp":"2026-03-05T20:35:00Z","message":"no agent"}]}]
    """).first)
    #expect(task.log.count == 2)
    #expect(task.log[0].agent == "claude")
    #expect(task.log[0].message == "did a thing")
    #expect(task.log[1].agent == nil)
}

// MARK: - Relations

@Test func candidatesBarWhatSdWouldRefuse() throws {
    // 1 → 2 → 3 as a dependency chain, and 4 is a child of 1.
    let graph = TaskGraph([
        try task(1, depends: [2], children: [4]),
        try task(2, depends: [3]),
        try task(3),
        try task(4, parent: 1),
    ])

    // Blocking 1 on anything already waiting on it would close a loop.
    let blockedBy = graph.candidates(for: 3, by: .blockedBy)
    #expect(blockedBy.first { $0.id == 2 }?.barred != nil)
    #expect(blockedBy.first { $0.id == 1 }?.barred != nil)
    #expect(blockedBy.first { $0.id == 4 }?.barred == nil)

    // A task cannot be nested inside its own descendant.
    let parents = graph.candidates(for: 1, by: .parent)
    #expect(parents.first { $0.id == 4 }?.barred != nil)
    #expect(parents.contains { $0.id == 1 } == false)
}

@Test func candidatesDropResolvedTasksButNotForParenting() throws {
    let graph = TaskGraph([try task(1), try task(2, status: .done)])
    // A dependency on a finished task is one `sd` strips, so the tick would
    // vanish the moment it was made.
    #expect(graph.candidates(for: 1, by: .blockedBy).contains { $0.id == 2 } == false)
    // Nesting under a finished task is allowed, so it stays on offer.
    #expect(graph.candidates(for: 1, by: .parent).contains { $0.id == 2 })
}

@Test func aFinishedTaskHasNothingToDependOn() throws {
    let graph = TaskGraph([try task(1, status: .done), try task(2)])
    #expect(graph.candidates(for: 1, by: .blockedBy).isEmpty)
    #expect(graph.emptiness(for: 1, by: .blockedBy) == "A finished task has no dependencies.")
    #expect(graph.emptiness(for: 1, by: .parent) == "Nothing to pick.")
}

@Test func aRelationIsOneEdgeReadFromEitherEnd() throws {
    let graph = TaskGraph([try task(1, depends: [2]), try task(2)])
    #expect(graph.isRelated(2, to: 1, by: .blockedBy))
    #expect(graph.isRelated(1, to: 2, by: .blocks))
    #expect(graph.blockedBy(1).map(\.id) == [2])
    #expect(graph.blocking(2).map(\.id) == [1])
}

@Test func relationEditsPickTheTaskTheEdgeIsStoredOn() {
    // "Blocks" is written on the other task, which is the whole reason this
    // returns which task to write rather than only what to write.
    let blocks = Relation.blocks.edit(9, to: 1, on: true)
    #expect(blocks.task == 9)
    #expect(blocks.edit.arguments == ["--add-dep=1"])

    let unparent = Relation.parent.edit(9, to: 1, on: false)
    #expect(unparent.task == 1)
    #expect(unparent.edit.arguments == ["--no-parent"])
}

@Test func searchMatchesAnIdWithOrWithoutItsHash() throws {
    let found = try task(7)
    #expect(found.matches("#7"))
    #expect(found.matches("7"))
    #expect(found.matches("t7"))
    #expect(found.matches("8") == false)
}
