/// "just now", "5 min ago", "3 h ago", "1 day ago", "2 days ago": how long ago [ms]
/// (epoch milliseconds) was, as shown on the Devices and Google pages.
String agoText(int ms, {DateTime? now}) {
  final s = ((now ?? DateTime.now()).millisecondsSinceEpoch - ms) ~/ 1000;
  if (s < 60) return 'just now';
  if (s < 3600) return '${s ~/ 60} min ago';
  if (s < 86400) return '${s ~/ 3600} h ago';
  final days = s ~/ 86400;
  return days == 1 ? '1 day ago' : '$days days ago';
}
