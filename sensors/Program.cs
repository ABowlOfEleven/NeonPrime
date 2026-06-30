// NeonPrime.Sensors — a thin sidecar that exposes LibreHardwareMonitor sensor
// readings to the Rust app as newline-delimited JSON on stdout.
//
// CPU package temperature, motherboard sensors, and fan speeds require LHM's
// kernel driver, which loads only when this process is elevated. GPU sensors
// (NVIDIA / AMD / Intel) work without elevation. Either way it degrades safely.

using System.Text.Json;

using LibreHardwareMonitor.Hardware;

namespace NeonPrime.Sensors;

internal sealed class UpdateVisitor : IVisitor
{
    public void VisitComputer(IComputer computer) => computer.Traverse(this);

    public void VisitHardware(IHardware hardware)
    {
        hardware.Update();
        foreach (IHardware sub in hardware.SubHardware)
        {
            sub.Accept(this);
        }
    }

    public void VisitSensor(ISensor sensor) { }
    public void VisitParameter(IParameter parameter) { }
}

internal sealed record SensorReading(string Hw, string HwType, string Name, string Type, float Value);

internal static class Program
{
    // `--out <path>` writes each snapshot to a file (so an unelevated parent can
    // read an elevated sidecar's output). With no `--out` it prints to stdout.
    // `--interval <ms>` sets the poll period.
    private static void Main(string[] args)
    {
        string? outPath = null;
        int interval = 1000;
        for (int i = 0; i < args.Length; i++)
        {
            if (args[i] == "--out" && i + 1 < args.Length)
            {
                outPath = args[++i];
            }
            else if (args[i] == "--interval" && i + 1 < args.Length && int.TryParse(args[i + 1], out int ms))
            {
                interval = ms;
                i++;
            }
        }

        var computer = new Computer
        {
            IsCpuEnabled = true,
            IsGpuEnabled = true,
            IsMotherboardEnabled = true,
            IsControllerEnabled = true,
            IsMemoryEnabled = false,
            IsStorageEnabled = false,
        };

        try
        {
            computer.Open();
        }
        catch
        {
            // Driver load can fail without elevation; GPU sensors still work.
        }

        var visitor = new UpdateVisitor();
        var options = new JsonSerializerOptions { PropertyNamingPolicy = JsonNamingPolicy.CamelCase };

        while (true)
        {
            computer.Accept(visitor);

            var readings = new List<SensorReading>();
            foreach (IHardware hw in computer.Hardware)
            {
                Collect(hw, readings);
            }
            string json = JsonSerializer.Serialize(readings, options);

            if (outPath is not null)
            {
                try
                {
                    string tmp = outPath + ".tmp";
                    File.WriteAllText(tmp, json);
                    File.Move(tmp, outPath, overwrite: true); // atomic replace
                }
                catch
                {
                    // Ignore transient IO races; try again next tick.
                }
            }
            else
            {
                // No --out: print one snapshot and exit. The app always runs us
                // with --out (a polled file), so the streaming loop only applies
                // there. A bare run must terminate so installer/AV validators
                // that launch the binary don't hang forever.
                Console.Out.WriteLine(json);
                Console.Out.Flush();
                return;
            }

            Thread.Sleep(interval);
        }
    }

    private static void Collect(IHardware hw, List<SensorReading> outList)
    {
        foreach (ISensor s in hw.Sensors)
        {
            if (s.Value is float v && !float.IsNaN(v) && !float.IsInfinity(v))
            {
                outList.Add(new SensorReading(
                    hw.Name,
                    hw.HardwareType.ToString(),
                    s.Name,
                    s.SensorType.ToString(),
                    v));
            }
        }
        foreach (IHardware sub in hw.SubHardware)
        {
            Collect(sub, outList);
        }
    }
}
