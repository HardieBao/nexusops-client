import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

const readRootFile = (filePath: string) =>
  readFileSync(path.resolve(process.cwd(), filePath), "utf8");

describe("NexusOps Client branding and protocol", () => {
  it("registers only the nexusops deep-link scheme", () => {
    const config = JSON.parse(readRootFile("src-tauri/tauri.conf.json"));
    const windowsConfig = JSON.parse(
      readRootFile("src-tauri/tauri.windows.conf.json"),
    );
    const infoPlist = readRootFile("src-tauri/Info.plist");
    const cargoManifest = readRootFile("src-tauri/Cargo.toml");

    expect(config.productName).toBe("NexusOps Client");
    expect(config.identifier).toBe("io.nexusops.client");
    expect(config.plugins["deep-link"].desktop.schemes).toEqual(["nexusops"]);
    expect(windowsConfig.app.windows[0].title).toBe("NexusOps Client");
    expect(infoPlist).toContain("<string>NexusOps Client Deep Link</string>");
    expect(infoPlist).toContain("<string>nexusops</string>");
    expect(infoPlist).not.toContain("<string>ccswitch</string>");
    expect(cargoManifest).toMatch(
      /\[\[bin\]\]\s+name = "nexusops-client"\s+path = "src\/main\.rs"/,
    );
  });

  it("keeps the runtime and local deep-link test page on nexusops", () => {
    const runtime = readRootFile("src-tauri/src/lib.rs");
    const testPage = readRootFile("deplink.html");
    const tray = readRootFile("src-tauri/src/tray.rs");

    expect(runtime).toContain('url_str.starts_with("nexusops://")');
    expect(runtime).not.toContain('url_str.starts_with("ccswitch://")');
    expect(runtime).toContain('app.deep_link().is_registered("nexusops")');
    expect(testPage).toContain("nexusops://v1/import");
    expect(testPage).not.toContain("ccswitch://");
    expect(testPage).toContain("url.protocol !== 'nexusops:'");
    expect(testPage).not.toContain("url.protocol !== 'ccswitch:'");
    expect(tray).toContain("https://github.com/HardieBao/nexusops-client");
    expect(tray).not.toContain("https://ccswitch.io");
  });

  it.each(["en", "ja", "zh-TW", "zh"])(
    "uses the NexusOps Client name in %s generic copy",
    (locale) => {
      const messages = JSON.parse(
        readRootFile(`src/i18n/locales/${locale}.json`),
      );
      const { partnerPromotion, ...providerForm } = messages.providerForm;
      const genericMessages = { ...messages, providerForm };

      expect(messages.app.title).toBe("NexusOps Client");
      expect(JSON.stringify(genericMessages)).not.toContain("CC Switch");
      expect(JSON.stringify(partnerPromotion)).toContain("CC Switch");
    },
  );
});
