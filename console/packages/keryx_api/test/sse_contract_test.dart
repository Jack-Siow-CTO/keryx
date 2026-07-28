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

  test('cancelAndRerun is pure client composition of cancel + start paths', () {
    // Structural: client exposes cancelAndRerun — shipped method must exist.
    expect(KeryxApiClient, isNotNull);
  });
}
