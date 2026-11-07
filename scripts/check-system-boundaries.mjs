import { readFile, readdir } from 'node:fs/promises';
import path from 'node:path';

// Tokenize strings/comments before balancing Rust items. In particular, braces
// in raw fixture strings must not hide production code after a cfg(test) item.
export function tokens(source) {
  const result = [];
  let i = 0;
  while (i < source.length) {
    const rest = source.slice(i);
    if (/^\s/.test(rest)) {
      i++;
      continue;
    }
    if (rest.startsWith('//')) {
      const end = source.indexOf('\n', i);
      i = end < 0 ? source.length : end;
      continue;
    }
    if (rest.startsWith('/*')) {
      let depth = 1;
      i += 2;
      while (i < source.length && depth) {
        if (source.startsWith('/*', i)) {
          depth++;
          i += 2;
        } else if (source.startsWith('*/', i)) {
          depth--;
          i += 2;
        } else i++;
      }
      continue;
    }
    const raw = /^(?:br|r)(#*)"/.exec(rest);
    const quoted = /^(?:b|c)?"/.exec(rest);
    if (raw || quoted) {
      const start = i;
      if (raw) {
        const end = source.indexOf(`"${raw[1]}`, i + raw[0].length);
        i = end < 0 ? source.length : end + 1 + raw[1].length;
      } else {
        i += quoted[0].length;
        while (i < source.length) {
          if (source[i] === '\\') i += 2;
          else if (source[i++] === '"') break;
        }
      }
      result.push({ value: source.slice(start, i), string: true });
      continue;
    }
    const char = /^(?:b)?'(?:\\(?:u\{[^}]+\}|x[0-9a-fA-F]{2}|.)|[^'\\])'/u.exec(rest);
    if (char) {
      i += char[0].length;
      continue;
    }
    const word = /^[A-Za-z_][A-Za-z_0-9]*/.exec(rest);
    if (word) {
      result.push({ value: word[0] });
      i += word[0].length;
    } else result.push({ value: source[i++] });
  }
  return result;
}

export function production(source) {
  const input = tokens(source);
  const result = [];
  for (let i = 0; i < input.length;) {
    if (
      input
        .slice(i, i + 7)
        .map((t) => t.value)
        .join('') === '#[cfg(test)]'
    ) {
      i += 7;
      // Skip other attributes on the same test-only item.
      while (input[i]?.value === '#' && input[i + 1]?.value === '[') {
        i += 2;
        let depth = 1;
        while (i < input.length && depth) {
          const token = input[i++];
          if (!token.string && token.value === '[') depth++;
          if (!token.string && token.value === ']') depth--;
        }
      }
      while (i < input.length && (input[i].string || !['{', ';'].includes(input[i].value))) i++;
      if (input[i]?.value === '{') {
        i++;
        let depth = 1;
        while (i < input.length && depth) {
          const token = input[i++];
          if (!token.string && token.value === '{') depth++;
          if (!token.string && token.value === '}') depth--;
        }
      } else i++;
    } else result.push(input[i++]);
  }
  return result;
}

export function violations(source, file) {
  const live = production(source);
  const code = live
    .filter((t) => !t.string)
    .map((t) => t.value)
    .join(' ');
  const errors = [];
  if (
    /\b(?:libc|gtk|gdk|gio|webkit2gtk|portable_pty|secret_service)\b/.test(code) ||
    /\b(?:gtk_window|with_webview|as_raw_fd|from_raw_fd|custom_flags|pre_exec|process_group)\b/.test(
      code,
    ) ||
    /\bos\s*:\s*:\s*(?:\{[^}]*\b(?:unix|linux|fd|windows)\b|(?:unix|linux|fd|windows)\b)/.test(
      code,
    ) ||
    /\bprocess\s*:\s*:\s*(?:\{[^}]*\b(?:Command|Stdio|Child)\b|(?:Command|Stdio|Child)\b)/.test(
      code,
    )
  ) {
    errors.push('native imports or calls belong in system/linux');
  }
  if (
    live.some(
      (t) =>
        t.string &&
        /^(?:b|c|(?:br|r)#*)?"\/(?:proc|sys|run|dev|bin|usr\/bin)(?:\/|"|$)/.test(t.value),
    )
  ) {
    errors.push('Linux system paths belong in system/linux');
  }
  if (file !== 'src-tauri/src/system/mod.rs' && /\btarget_os\b/.test(code)) {
    errors.push('backend selection belongs in system/mod.rs');
  }
  const entry = file === 'src-tauri/src/main.rs' || file.startsWith('src-tauri/src/bin/');
  if (!entry && /\bsystem\s*:\s*:\s*(?:services\b|\{[^}]*\bservices\b)/.test(code)) {
    errors.push('receive services through injection, not the backend factory');
  }
  return errors;
}

async function files(directory) {
  const entries = await readdir(directory, { withFileTypes: true });
  return (
    await Promise.all(
      entries.map((entry) =>
        entry.isDirectory()
          ? files(path.join(directory, entry.name))
          : [path.join(directory, entry.name)],
      ),
    )
  ).flat();
}

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename)) {
  const failures = [];
  for (const file of await files('src-tauri/src')) {
    if (!file.endsWith('.rs') || file.startsWith('src-tauri/src/system/linux/')) continue;
    // These modules are only reached through cfg(test) declarations.
    if (file.endsWith('/tests.rs') || file.endsWith('_tests.rs') || file.includes('/tests/'))
      continue;
    for (const error of violations(await readFile(file, 'utf8'), file))
      failures.push(`${file}: ${error}`);
  }
  if (failures.length) {
    console.error(failures.join('\n'));
    process.exitCode = 1;
  } else console.log('System boundaries passed');
}
