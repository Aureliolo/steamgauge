// The DLLs a Windows release has to carry beside its program, read from what the program imports.
//
//   node tools/release/runtime-dlls.mjs place <folder> <redist folder>
//   node tools/release/runtime-dlls.mjs check <folder>
//
// steamgauge.exe and the libraries beside it name the DLLs they import, and Windows refuses to
// start a program one of them is missing for (STATUS_DLL_NOT_FOUND) before any of its code runs.
// Windows' own DLLs are on every machine. Any other has to ship in the program's folder. The C++
// runtime is the one the build needs that nothing else puts there: ONNX Runtime's prebuilt library
// is compiled against it, and a Windows that never installed the Visual C++ Redistributable has
// none. A machine that has it, as every development machine and CI runner does, starts the
// program either way, which is why this is read from the files rather than found by running them.
//
// `place` copies into <folder> every DLL its programs and libraries import, and those import in
// turn, that the Visual C++ redistributable folder holds (VC\Redist\MSVC\<version>\x64\
// Microsoft.VC<toolset>.CRT of the Visual Studio that built them). `check` refuses a folder in
// which anything imports a DLL that is neither in it nor in WINDOWS below.
import { copyFileSync, readdirSync, readFileSync } from 'node:fs';
import { join } from 'node:path';
import process from 'node:process';

import { isMain } from './github.mjs';

const EXE_OR_DLL_FILE = /\.(?:exe|dll)$/iu;

// The DLLs Windows 10 and 11 carry in System32, of those the release's files import. A name
// added here is one checked to ship with Windows itself, never with a redistributable or a driver.
// API sets (api-ms-win-*, ext-ms-*) are names Windows resolves to its own libraries.
const WINDOWS = new Set([
  'advapi32.dll',
  'bcrypt.dll',
  'bcryptprimitives.dll',
  'combase.dll',
  'comctl32.dll',
  'crypt32.dll',
  'd3d12.dll',
  'dbghelp.dll',
  'dwmapi.dll',
  'dxcore.dll',
  'dxgi.dll',
  'gdi32.dll',
  'imm32.dll',
  'kernel32.dll',
  'ntdll.dll',
  'ole32.dll',
  'oleaut32.dll',
  'setupapi.dll',
  'shell32.dll',
  'shlwapi.dll',
  'user32.dll',
  'ws2_32.dll',
]);

function isWindows(name) {
  const lower = name.toLowerCase();
  return WINDOWS.has(lower) || lower.startsWith('api-ms-win-') || lower.startsWith('ext-ms-');
}

// Where the section table and the data directories sit, after the signatures check out.
function peLayout(view) {
  if (view.getUint16(0, true) !== 0x5a_4d) {
    throw new Error('not a PE file: no MZ header');
  }
  const pe = view.getUint32(0x3c, true);
  if (view.getUint32(pe, true) !== 0x45_50) {
    throw new Error('not a PE file: no PE signature');
  }
  const optional = pe + 24;
  const magic = view.getUint16(optional, true);
  if (magic !== 0x1_0b && magic !== 0x2_0b) {
    throw new Error(`not a PE file: optional header magic ${magic.toString(16)}`);
  }
  const wide = magic === 0x2_0b;
  return {
    directories: optional + (wide ? 112 : 96),
    directoryCount: view.getUint32(optional + (wide ? 108 : 92), true),
    sections: view.getUint16(pe + 6, true),
    tableAt: optional + view.getUint16(pe + 20, true),
  };
}

function sectionTable(view, { sections, tableAt }) {
  const table = [];
  for (let index = 0; index < sections; index += 1) {
    const at = tableAt + index * 40;
    table.push({
      address: view.getUint32(at + 12, true),
      offset: view.getUint32(at + 20, true),
      size: Math.max(view.getUint32(at + 8, true), view.getUint32(at + 16, true)),
    });
  }
  return table;
}

// The DLL names a PE file imports, directly and delay-loaded, in the order its tables give them.
function imports(bytes) {
  const view = new DataView(bytes.buffer, bytes.byteOffset, bytes.byteLength);
  const layout = peLayout(view);
  const table = sectionTable(view, layout);
  const offsetOf = (rva) => {
    const section = table.find((entry) => rva >= entry.address && rva < entry.address + entry.size);
    if (section === undefined) {
      throw new Error(`RVA ${rva.toString(16)} is in no section`);
    }
    return rva - section.address + section.offset;
  };
  const nameAt = (rva) => {
    const start = offsetOf(rva);
    const end = bytes.indexOf(0, start);
    return new TextDecoder('latin1').decode(bytes.subarray(start, end));
  };
  const directory = (index) =>
    index < layout.directoryCount ? view.getUint32(layout.directories + index * 8, true) : 0;
  // `stride` is the size of one descriptor and `field` where it holds the name's RVA.
  const namesIn = (index, stride, field) => {
    const names = [];
    const start = directory(index);
    if (start === 0) {
      return names;
    }
    for (let at = offsetOf(start); ; at += stride) {
      const name = view.getUint32(at + field, true);
      if (name === 0) {
        return names;
      }
      names.push(nameAt(name));
    }
  };

  return [...namesIn(1, 20, 12), ...namesIn(13, 32, 4)];
}

function programs(folder) {
  return readdirSync(folder).filter((file) => EXE_OR_DLL_FILE.test(file));
}

// Copies what <folder>'s files import from <redist>, until nothing there imports anything more
// from it. Returns the names copied.
function place(folder, redist) {
  const available = new Map(readdirSync(redist).map((file) => [file.toLowerCase(), file]));
  const present = new Set(programs(folder).map((file) => file.toLowerCase()));
  const queue = programs(folder);
  const placed = [];
  while (queue.length > 0) {
    const file = queue.shift();
    for (const name of imports(readFileSync(join(folder, file)))) {
      const lower = name.toLowerCase();
      if (present.has(lower) || !available.has(lower)) {
        continue;
      }
      const copied = available.get(lower);
      copyFileSync(join(redist, copied), join(folder, copied));
      present.add(lower);
      placed.push(copied);
      queue.push(copied);
    }
  }
  return placed;
}

// What in <folder> imports a DLL that is neither beside it nor Windows' own, one line each.
function check(folder) {
  const files = programs(folder);
  const present = new Set(files.map((file) => file.toLowerCase()));
  const problems = [];
  for (const file of files) {
    for (const name of imports(readFileSync(join(folder, file)))) {
      if (!(present.has(name.toLowerCase()) || isWindows(name))) {
        problems.push(`${file} imports ${name}, which is neither beside it nor part of Windows.`);
      }
    }
  }
  return problems;
}

if (isMain(import.meta.url)) {
  const [command, folder, redist] = process.argv.slice(2);
  if (command === 'place' && folder !== undefined && redist !== undefined) {
    for (const copied of place(folder, redist)) {
      console.log(copied);
    }
  } else if (command === 'check' && folder !== undefined) {
    const problems = check(folder);
    for (const problem of problems) {
      console.error(problem);
    }
    if (problems.length > 0) {
      console.error(
        "A DLL that is not Windows' own has to ship beside the program; one that is goes into WINDOWS in tools/release/runtime-dlls.mjs.",
      );
      process.exitCode = 1;
    }
  } else {
    console.error('usage: runtime-dlls.mjs place <folder> <redist folder> | check <folder>');
    process.exitCode = 2;
  }
}

export { check, imports, isWindows, place, WINDOWS };
