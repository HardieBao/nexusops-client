const DOWNLOAD_RULES = [
  { suffix: "-macOS.dmg", platform: "macos", kind: "dmg", arch: "universal" },
  { suffix: "-macOS.zip", platform: "macos", kind: "zip", arch: "universal" },
  {
    suffix: "-Windows-arm64-Portable.zip",
    platform: "windows",
    kind: "portable",
    arch: "arm64",
  },
  {
    suffix: "-Windows-Portable.zip",
    platform: "windows",
    kind: "portable",
    arch: "x64",
  },
  {
    suffix: "-Windows-arm64.msi",
    platform: "windows",
    kind: "msi",
    arch: "arm64",
  },
  { suffix: "-Windows.msi", platform: "windows", kind: "msi", arch: "x64" },
  {
    suffix: "-Linux-arm64.AppImage",
    platform: "linux",
    kind: "appimage",
    arch: "arm64",
  },
  {
    suffix: "-Linux-x86_64.AppImage",
    platform: "linux",
    kind: "appimage",
    arch: "x64",
  },
  { suffix: "-Linux-arm64.deb", platform: "linux", kind: "deb", arch: "arm64" },
  { suffix: "-Linux-x86_64.deb", platform: "linux", kind: "deb", arch: "x64" },
  { suffix: "-Linux-arm64.rpm", platform: "linux", kind: "rpm", arch: "arm64" },
  { suffix: "-Linux-x86_64.rpm", platform: "linux", kind: "rpm", arch: "x64" },
];

export function getReleaseAssetContract(tag) {
  const prefix = `NexusOps-Client-${tag}`;
  const name = (suffix) => `${prefix}${suffix}`;
  const downloads = DOWNLOAD_RULES.map((rule) => ({
    ...rule,
    name: name(rule.suffix),
  }));

  const buildEvidence = [
    descriptor(prefix, "Windows", "x86_64", [
      name("-Windows.msi"),
      name("-Windows.msi.sig"),
      name("-Windows-Portable.zip"),
    ]),
    descriptor(prefix, "Windows", "arm64", [
      name("-Windows-arm64.msi"),
      name("-Windows-arm64.msi.sig"),
      name("-Windows-arm64-Portable.zip"),
    ]),
    descriptor(prefix, "Linux", "x86_64", [
      name("-Linux-x86_64.AppImage"),
      name("-Linux-x86_64.AppImage.sig"),
      name("-Linux-x86_64.deb"),
      name("-Linux-x86_64.rpm"),
    ]),
    descriptor(prefix, "Linux", "arm64", [
      name("-Linux-arm64.AppImage"),
      name("-Linux-arm64.AppImage.sig"),
      name("-Linux-arm64.deb"),
      name("-Linux-arm64.rpm"),
    ]),
    descriptor(prefix, "macOS", "universal", [
      name("-macOS.dmg"),
      name("-macOS.zip"),
      name("-macOS.tar.gz"),
      name("-macOS.tar.gz.sig"),
    ]),
  ];

  const payloadNames = new Set([
    ...downloads.map((entry) => entry.name),
    name("-macOS.tar.gz"),
  ]);
  const requiredReleaseNames = new Set([
    ...buildEvidence.flatMap((entry) => [
      ...entry.artifactNames,
      entry.evidenceName,
      entry.sumsName,
    ]),
    ...payloadNames,
    "latest.json",
    "G3-EVIDENCE.json",
    "NexusOps-Client-LICENSE.txt",
    "NexusOps-Client-UPSTREAM-NOTICE.md",
  ]);

  return {
    prefix,
    downloads,
    buildEvidence,
    payloadNames,
    requiredReleaseNames,
  };
}

function descriptor(prefix, platform, architecture, files) {
  const id = `${prefix}-${platform}-${architecture}`;
  const toolchainName = `${id}-toolchain.txt`;
  return {
    id,
    platform,
    architecture,
    toolchainName,
    evidenceName: `${id}-build-evidence.json`,
    sumsName: `${id}-SHA256SUMS.txt`,
    artifactNames: [...files, toolchainName].sort((left, right) =>
      left.localeCompare(right),
    ),
  };
}
