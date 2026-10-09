using System.Diagnostics;
using System.Runtime.InteropServices;

namespace ClaudeCodeUI;

// Ties a child's whole process tree to the child: Dispose() kills every descendant still alive, even after the
// parent itself was killed from outside (Windows does not propagate a parent's death, and Kill(true) needs a live parent).
//  - Windows: Job Object with KILL_ON_JOB_CLOSE.
//  - macOS/Linux: the child is started in its own session (Prepare) and Dispose() SIGKILLs that process group.
// ponytail: start-up race on Windows (a child spawned before AssignProcessToJobObject escapes); upgrade path is
// CreateProcess with CREATE_SUSPENDED, which Process.Start cannot do. On Unix, a descendant that calls setsid itself escapes.
internal sealed class ProcessJob : IDisposable
{
    IntPtr handle;   // Windows job handle
    int pgid;        // Unix process group (= pid of the setsid'd child)

    ProcessJob() { }

    // Unix only: run the command through perl so it becomes a session/group leader (.NET cannot setsid). No-op elsewhere or without perl.
    public static void Prepare(ProcessStartInfo psi)
    {
        if (OperatingSystem.IsWindows() || !OnPath("perl")) return;
        var args = psi.ArgumentList.ToList();
        psi.ArgumentList.Clear();
        foreach (var a in new[] { "-e", "use POSIX; POSIX::setsid(); exec { $ARGV[0] } @ARGV or die \"exec $ARGV[0]: $!\"", psi.FileName }.Concat(args))
            psi.ArgumentList.Add(a);
        psi.FileName = "perl";
    }

    // null if the OS refuses (logged, never throws); the session then runs as before.
    public static ProcessJob? Attach(Process p)
    {
        try
        {
            if (!OperatingSystem.IsWindows()) return new ProcessJob { pgid = p.Id };
            var job = CreateJobObjectW(IntPtr.Zero, null);
            if (job == IntPtr.Zero) throw new System.ComponentModel.Win32Exception();
            var info = new ExtendedLimits { LimitFlags = 0x2000 };
            if (!SetInformationJobObject(job, 9, ref info, Marshal.SizeOf<ExtendedLimits>())
                || !AssignProcessToJobObject(job, p.Handle))
            {
                var err = Marshal.GetLastWin32Error();
                CloseHandle(job);
                throw new System.ComponentModel.Win32Exception(err);
            }
            return new ProcessJob { handle = job };
        }
        catch (Exception ex)
        {
            Console.Error.WriteLine($"ProcessJob: arbre de processus non suivi ({ex.Message})");
            return null;
        }
    }

    public void Dispose()
    {
        var h = Interlocked.Exchange(ref handle, IntPtr.Zero);
        if (h != IntPtr.Zero) CloseHandle(h);
        var g = Interlocked.Exchange(ref pgid, 0);
        if (g > 1) try { kill(-g, 9); } catch { }
    }

    static bool OnPath(string exe) =>
        (Environment.GetEnvironmentVariable("PATH") ?? "").Split(Path.PathSeparator, StringSplitOptions.RemoveEmptyEntries)
            .Any(d => File.Exists(Path.Combine(d, exe)));

    [StructLayout(LayoutKind.Sequential)]
    struct ExtendedLimits   // JOBOBJECT_EXTENDED_LIMIT_INFORMATION (only LimitFlags is set)
    {
        public long PerProcessUserTimeLimit, PerJobUserTimeLimit;
        public uint LimitFlags;
        public nuint MinimumWorkingSetSize, MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public nuint Affinity;
        public uint PriorityClass, SchedulingClass;
        public ulong ReadOps, WriteOps, OtherOps, ReadBytes, WriteBytes, OtherBytes;
        public nuint ProcessMemoryLimit, JobMemoryLimit, PeakProcessMemoryUsed, PeakJobMemoryUsed;
    }

    [DllImport("kernel32", SetLastError = true)] static extern IntPtr CreateJobObjectW(IntPtr attrs, string? name);
    [DllImport("kernel32", SetLastError = true)] static extern bool SetInformationJobObject(IntPtr job, int cls, ref ExtendedLimits info, int size);
    [DllImport("kernel32", SetLastError = true)] static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);
    [DllImport("kernel32")] static extern bool CloseHandle(IntPtr h);
    [DllImport("libc", SetLastError = true)] static extern int kill(int pid, int sig);

    // Kills only the parent of a two-level tree, disposes the job, and expects the grandchild (holder of the stdout pipe) gone.
    internal static void Check()
    {
        var win = OperatingSystem.IsWindows();
        var psi = win
            ? new ProcessStartInfo("cmd.exe", "/c \"ping -n 2 127.0.0.1 >NUL & ping -n 60 127.0.0.1\"")
            : new ProcessStartInfo("/bin/sh") { ArgumentList = { "-c", "sleep 60 & sleep 60" } };
        psi.RedirectStandardOutput = true;
        psi.UseShellExecute = false;
        Prepare(psi);
        using var p = Process.Start(psi)!;
        using var job = Attach(p);
        SelfCheck.Assert(job is not null, "ProcessJob: Attach returned null");
        Thread.Sleep(win ? 2500 : 500);
        p.Kill(false);
        job!.Dispose();
        SelfCheck.Assert(p.StandardOutput.ReadToEndAsync().Wait(TimeSpan.FromSeconds(5)), "ProcessJob: the grandchild survives");
    }
}
