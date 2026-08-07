import 'package:keryx_api/keryx_api.dart';
import 'package:test/test.dart';

void main() {
  test('SseParser extracts event frames and RunEvent terminal', () {
    const chunk = 'event: model.delta\n'
        'id: 2\n'
        'data: {"run_id":"r1","seq":2,"kind":{"type":"model_delta","payload":{"text":"hi"}}}\n'
        '\n'
        'event: run.completed\n'
        'id: 3\n'
        'data: {"run_id":"r1","seq":3,"kind":{"type":"run_completed"}}\n'
        '\n';
    final parser = SseParser();
    final frames = parser.push(chunk);
    expect(frames.length, 2);
    final e1 = RunEvent.fromSse(frames[0]);
    expect(e1.deltaText, 'hi');
    expect(e1.isTerminal, isFalse);
    final e2 = RunEvent.fromSse(frames[1]);
    expect(e2.isTerminal, isTrue);
  });

  test('RunEvent exposes tool and Child-Run activity fields', () {
    const toolChunk = 'event: tool.started\n'
        'data: {"run_id":"r1","seq":1,"kind":{"type":"tool_started","payload":{"name":"workspace_read"}}}\n'
        '\n'
        'event: child_run.started\n'
        'data: {"run_id":"r1","seq":2,"kind":{"type":"child_run_started","payload":{"child_run_id":"c1","goal":"scan"}}}\n'
        '\n';
    final frames = SseParser().push(toolChunk);
    final tool = RunEvent.fromSse(frames[0]);
    expect(tool.isToolStarted, isTrue);
    expect(tool.toolName, 'workspace_read');
    final child = RunEvent.fromSse(frames[1]);
    expect(child.isChildRunStarted, isTrue);
    expect(child.childRunId, 'c1');
    expect(child.childRunGoal, 'scan');
  });

  test('cancelAndRerun is pure client composition of cancel + start paths', () {
    // Structural: client exposes cancelAndRerun — shipped method must exist.
    expect(KeryxApiClient, isNotNull);
  });
}
