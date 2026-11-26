@Tags(['golden'])
library;

// ignore: deprecated_member_use
import 'package:flutter/material.dart';
import 'package:flutter_test/flutter_test.dart';
import 'package:utre/desk/devices.dart';
import 'package:utre/src/rust/api/tasks.dart';

import 'fake_db.dart';
import 'golden_utils.dart';

int minutesAgo(int m) =>
    DateTime.now().subtract(Duration(minutes: m)).millisecondsSinceEpoch;

List<Peer> peers() => [
  Peer(
    nodeId: '58385947aa01b2c3d4e5f60718293a4b5c6d7e8f9012a3b4c5d6e7f8091a2b3c',
    name: "Boris's MacBook Pro",
    addedAt: minutesAgo(60 * 24 * 3),
    lastSeen: minutesAgo(5),
    lastAddr: '192.168.1.24:41824',
  ),
  Peer(
    nodeId: '9f1e2d3c4b5a69788796a5b4c3d2e1f00112233445566778899aabbccddeeff0',
    name: 'Xiaomi 22101316G',
    addedAt: minutesAgo(60 * 24 * 2),
    lastSeen: minutesAgo(60 * 30),
  ),
  Peer(
    nodeId: '0a1b2c3d4e5f60718293a4b5c6d7e8f9012a3b4c5d6e7f8091a2b3c4d5e6f708',
    name: 'Old tablet',
    addedAt: minutesAgo(60 * 24 * 40),
  ),
];

void main() {
  useTolerantComparator();

  goldenTheme('devices', (t, b, file) async {
    setSurface(t, const Size(900, 700));
    await t.pumpWidget(
      themed(
        Scaffold(
          body: DevicesPage(db: FakeDb(peerList: peers())),
        ),
        b,
      ),
    );
    await t.pump();
    await t.pump(const Duration(milliseconds: 300));
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });

  goldenTheme('devices-empty', (t, b, file) async {
    setSurface(t, const Size(900, 520));
    await t.pumpWidget(themed(Scaffold(body: DevicesPage(db: FakeDb())), b));
    await t.pump();
    await t.pump(const Duration(milliseconds: 300));
    await expectLater(find.byType(MaterialApp), matchesGoldenFile(file));
  });
}
