// The CPU time a container has used so far, in microseconds: usage_usec from its cgroup's
// cpu.stat (cgroup v2). Read before and after a phase, it gives the phase's CPU exactly, where
// docker stats only samples a percentage once a second. On a Linux host the file is read
// directly; under Docker Desktop the cgroups live in its VM, so a helper container reads them.
import { existsSync, readFileSync } from "node:fs";
import { spawnSync } from "node:child_process";

const docker = (...args) => spawnSync("docker", args, { encoding: "utf8" });
const usage = (text) => {
  const m = /^usage_usec (\d+)$/m.exec(text ?? "");
  return m ? Number(m[1]) : null;
};

/** CPU microseconds the container has used, or null when its cgroup cannot be read. */
export function cpuUsec(container) {
  // Never throws: a counter that cannot be read costs this number, not the run.
  try {
    return read(container);
  } catch {
    return null;
  }
}

function read(container) {
  const id = docker("inspect", "-f", "{{.Id}}", container).stdout?.trim();
  if (!id) return null;
  // The systemd cgroup driver (Ubuntu, the bench droplets) and the cgroupfs driver.
  for (const dir of [`/sys/fs/cgroup/system.slice/docker-${id}.scope`, `/sys/fs/cgroup/docker/${id}`]) {
    if (existsSync(`${dir}/cpu.stat`)) return usage(readFileSync(`${dir}/cpu.stat`, "utf8"));
  }
  const find = `cat "$(find /sys/fs/cgroup -maxdepth 3 -type d -name '*${id}*' | head -n 1)/cpu.stat"`;
  return usage(docker("run", "--rm", "--cgroupns=host", "-v", "/sys/fs/cgroup:/sys/fs/cgroup:ro", "alpine", "sh", "-c", find).stdout);
}
