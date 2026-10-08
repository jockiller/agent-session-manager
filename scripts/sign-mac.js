#!/usr/bin/env node
/**
 * macOS 代码签名与校验脚本 (对齐 gao-tool 打包签名逻辑)
 * 
 * 核心逻辑：
 * 1. 递归扫描 .app 内所有 Mach-O 二进制文件（底层可执行文件、动态库、资源二进制）；
 * 2. 对每个 Mach-O 文件独立执行 codesign；
 * 3. 附带 Hardened Runtime (--options runtime) 与 entitlements.plist 权限声明；
 * 4. 对顶层 .app 执行 --deep 深度兜底签名；
 * 5. 执行 codesign --verify --deep --strict 严格验证；
 * 6. 若存在对应 .dmg，也对 .dmg 文件进行签名与验证。
 */

import { spawnSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const __filename = fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);
const projectRoot = path.resolve(__dirname, '..');

function run(cmd, args, options = {}) {
  const printableCmd = `${cmd} ${args.join(' ')}`;
  console.log(`[exec] ${printableCmd}`);
  const result = spawnSync(cmd, args, {
    stdio: 'inherit',
    cwd: options.cwd || projectRoot,
    ...options
  });
  if (result.status !== 0) {
    throw new Error(`Command failed with code ${result.status}: ${printableCmd}`);
  }
}

function isMachO(filePath) {
  try {
    const fd = fs.openSync(filePath, 'r');
    const buf = Buffer.alloc(4);
    fs.readSync(fd, buf, 0, 4, 0);
    fs.closeSync(fd);

    const magicBE = buf.readUInt32BE(0);
    const magicLE = buf.readUInt32LE(0);
    return (
      magicBE === 0xfeedface || // Mach-O 32-bit
      magicBE === 0xfeedfacf || // Mach-O 64-bit
      magicBE === 0xcafebabe || // Universal Fat Binary
      magicLE === 0xfeedface ||
      magicLE === 0xfeedfacf ||
      magicLE === 0xcafebabe
    );
  } catch {
    return false;
  }
}

function walkFiles(dir, out = []) {
  if (!fs.existsSync(dir)) return out;
  const entries = fs.readdirSync(dir, { withFileTypes: true });
  for (const entry of entries) {
    if (entry.isSymbolicLink()) continue;
    const fullPath = path.join(dir, entry.name);
    if (entry.isDirectory()) {
      if (entry.name === '_CodeSignature') continue;
      walkFiles(fullPath, out);
    } else if (entry.isFile()) {
      out.push(fullPath);
    }
  }
  return out;
}

export function signMacBundle(targetAppPath = null) {
  if (process.platform !== 'darwin') {
    console.log('[sign-mac] 当前非 macOS 环境，跳过 macOS 代码签名。');
    return;
  }

  // 1. 自动寻找 .app 路径
  let appPath = targetAppPath;
  if (!appPath) {
    const defaultBundleDir = path.join(projectRoot, 'src-tauri/target/release/bundle/macos');
    if (fs.existsSync(defaultBundleDir)) {
      const candidates = fs.readdirSync(defaultBundleDir).filter(f => f.endsWith('.app'));
      if (candidates.length > 0) {
        appPath = path.join(defaultBundleDir, candidates[0]);
      }
    }
  }

  if (!appPath || !fs.existsSync(appPath)) {
    console.error(`[sign-mac] 未找到有效的 .app 安装包: ${appPath || '(无路径)'}`);
    console.error('[sign-mac] 请先执行 `pnpm tauri build`，或指定 .app 绝对路径。');
    process.exit(1);
  }

  console.log(`\n========================================`);
  console.log(`[sign-mac] 开始对应用进行代码签名: ${path.basename(appPath)}`);
  console.log(`[sign-mac] 路径: ${appPath}`);
  console.log(`========================================\n`);

  // 2. 确定签名证书与 entitlements
  const identity = process.env.APPLE_SIGNING_IDENTITY || '-';
  const isAdHoc = identity === '-';
  const entitlementsPath = path.join(projectRoot, 'src-tauri/entitlements.plist');
  const hasEntitlements = fs.existsSync(entitlementsPath);

  console.log(`[sign-mac] 签名身份: ${identity} (${isAdHoc ? 'ad-hoc 自签名' : 'Developer ID'})`);
  console.log(`[sign-mac] Entitlements: ${hasEntitlements ? entitlementsPath : '未配置'}`);

  // 3. 递归扫描并独立签名内部 Mach-O 文件
  const contentsDir = path.join(appPath, 'Contents');
  const allFiles = walkFiles(contentsDir);
  const machOFiles = allFiles.filter(isMachO);

  console.log(`[sign-mac] 找到 ${machOFiles.length} 个 Mach-O 独立二进制文件，开始逐一签名...`);
  for (const file of machOFiles) {
    const rel = path.relative(appPath, file);
    const args = ['--force', '--sign', identity];
    if (isAdHoc) {
      args.push('--timestamp=none');
    }
    // 对主可执行程序附加 entitlements
    if (file.includes('Contents/MacOS/') && hasEntitlements) {
      args.push('--options', 'runtime', '--entitlements', entitlementsPath);
    }
    args.push(file);
    console.log(`[sign-mac] 正在签名: ${rel}`);
    run('codesign', args);
  }

  // 4. 对整个 .app 做 --deep 兜底签名
  console.log(`\n[sign-mac] 执行顶层 --deep 深度签名...`);
  const topArgs = ['--force', '--deep', '--sign', identity];
  if (isAdHoc) {
    topArgs.push('--timestamp=none');
  }
  if (hasEntitlements) {
    topArgs.push('--options', 'runtime', '--entitlements', entitlementsPath);
  }
  topArgs.push(appPath);
  run('codesign', topArgs);

  // 5. 严格验证签名有效性
  console.log(`\n[sign-mac] 验证 .app 签名有效性...`);
  run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', appPath]);
  run('codesign', ['-dv', '--verbose=4', appPath]);
  console.log(`\n✓ [sign-mac] .app 签名及验证成功！\n`);

  // 6. 如果存在 DMG 文件，同样对 DMG 文件执行签名
  const dmgDir = path.join(projectRoot, 'src-tauri/target/release/bundle/dmg');
  if (fs.existsSync(dmgDir)) {
    const dmgs = fs.readdirSync(dmgDir).filter(f => f.endsWith('.dmg'));
    for (const dmg of dmgs) {
      const dmgPath = path.join(dmgDir, dmg);
      console.log(`[sign-mac] 正在对 DMG 文件签名: ${dmg}`);
      const dmgArgs = ['--force', '--sign', identity];
      if (isAdHoc) {
        dmgArgs.push('--timestamp=none');
      }
      dmgArgs.push(dmgPath);
      run('codesign', dmgArgs);
      run('codesign', ['--verify', '--deep', '--strict', '--verbose=2', dmgPath]);
      console.log(`✓ [sign-mac] DMG 签名及验证成功: ${dmg}`);
    }
  }
}

// CLI 直接调用执行
if (process.argv[1] === __filename) {
  const customPath = process.argv[2] ? path.resolve(process.argv[2]) : null;
  signMacBundle(customPath);
}
