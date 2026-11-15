// Минимальный разбор tar (ustar/GNU/PAX): имя файла → содержимое
export function readTar(buf) {
  const out = new Map();
  let off = 0;
  let longName = null;
  while (off + 512 <= buf.length) {
    const h = buf.subarray(off, off + 512);
    if (h.every((b) => b === 0)) break;
    const str = (a, b) => h.subarray(a, b).toString('utf8').replace(/\0[\s\S]*$/, '');
    const size = parseInt(str(124, 136).trim() || '0', 8);
    const type = h[156] === 0 ? '0' : String.fromCharCode(h[156]);
    // POSIX ustar хранит префикс пути отдельно; у GNU это поле занято датами
    const posix = h.subarray(257, 263).toString('latin1') === 'ustar\0';
    const prefix = posix ? str(345, 500) : '';
    const name = prefix ? `${prefix}/${str(0, 100)}` : str(0, 100);
    const data = buf.subarray(off + 512, off + 512 + size);
    off += 512 + Math.ceil(size / 512) * 512;
    if (type === 'L') {
      longName = data.toString('utf8').replace(/\0+$/, '');
      continue;
    }
    if (type === 'x') {
      const m = data.toString('utf8').match(/\d+ path=([^\n]*)\n/);
      if (m) longName = m[1];
      continue;
    }
    if (type === '0' || type === '7') out.set(longName || name, data);
    longName = null;
  }
  return out;
}
