import { useCompressionStore } from "../../stores/compressionStore";
import type { VideoCodec, AudioCodec } from "../../types/compression";

const CODECS: { value: VideoCodec; label: string; description: string }[] = [
  { value: "H264", label: "H.264", description: "Best compatibility" },
  { value: "H265", label: "H.265 / HEVC", description: "Better compression" },
  { value: "AV1", label: "AV1", description: "Best compression, slower" },
];

const RESOLUTIONS = [
  { label: "Original", value: null },
  { label: "4K (2160p)", value: { width: 3840, height: 2160 } },
  { label: "1080p", value: { width: 1920, height: 1080 } },
  { label: "720p", value: { width: 1280, height: 720 } },
  { label: "480p", value: { width: 854, height: 480 } },
];

const FRAMERATES = [
  { label: "Original", value: null },
  { label: "60 fps", value: 60 },
  { label: "30 fps", value: 30 },
  { label: "24 fps", value: 24 },
  { label: "15 fps", value: 15 },
];

const AUDIO_CODECS: { value: AudioCodec; label: string }[] = [
  { value: "AAC", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Copy", label: "Copy Original" },
  { value: "None", label: "No Audio" },
];

const AUDIO_BITRATES = ["64k", "96k", "128k", "192k", "256k", "320k"];

export function VideoControls() {
  const options = useCompressionStore((s) => s.videoOptions);
  const setOptions = useCompressionStore((s) => s.setVideoOptions);

  return (
    <div className="space-y-5">
      <SectionLabel>Video Settings</SectionLabel>

      {/* Codec */}
      <FieldGroup label="Codec">
        <div className="grid grid-cols-3 gap-1.5">
          {CODECS.map((c) => (
            <ChipButton
              key={c.value}
              active={options.codec === c.value}
              onClick={() => setOptions({ codec: c.value })}
            >
              {c.label}
            </ChipButton>
          ))}
        </div>
      </FieldGroup>

      {/* CRF / Quality */}
      <FieldGroup label="Quality (CRF)" trailing={<span className="font-data">{options.crf}</span>}>
        <input
          type="range"
          min={0}
          max={51}
          value={options.crf}
          onChange={(e) => setOptions({ crf: Number(e.target.value) })}
          className="w-full"
        />
        <div className="mt-1 flex justify-between text-[10px]" style={{ color: "var(--text-muted)" }}>
          <span>Higher quality</span>
          <span>Smaller file</span>
        </div>
      </FieldGroup>

      {/* Resolution */}
      <FieldGroup label="Resolution">
        <SelectInput
          value={options.resolution ? `${options.resolution.width}x${options.resolution.height}` : "original"}
          onChange={(e) => {
            if (e.target.value === "original") {
              setOptions({ resolution: null });
            } else {
              const res = RESOLUTIONS.find(
                (r) => r.value && `${r.value.width}x${r.value.height}` === e.target.value,
              );
              if (res?.value) setOptions({ resolution: res.value });
            }
          }}
        >
          {RESOLUTIONS.map((r) => (
            <option key={r.label} value={r.value ? `${r.value.width}x${r.value.height}` : "original"}>
              {r.label}
            </option>
          ))}
        </SelectInput>
      </FieldGroup>

      {/* Frame Rate */}
      <FieldGroup label="Frame Rate">
        <SelectInput
          value={options.framerate ?? "original"}
          onChange={(e) =>
            setOptions({
              framerate: e.target.value === "original" ? null : Number(e.target.value),
            })
          }
        >
          {FRAMERATES.map((f) => (
            <option key={f.label} value={f.value ?? "original"}>
              {f.label}
            </option>
          ))}
        </SelectInput>
      </FieldGroup>

      {/* Audio */}
      <FieldGroup label="Audio Codec">
        <SelectInput
          value={options.audioCodec}
          onChange={(e) => setOptions({ audioCodec: e.target.value as AudioCodec })}
        >
          {AUDIO_CODECS.map((a) => (
            <option key={a.value} value={a.value}>
              {a.label}
            </option>
          ))}
        </SelectInput>
      </FieldGroup>

      {options.audioCodec !== "None" && options.audioCodec !== "Copy" && (
        <FieldGroup label="Audio Bitrate">
          <SelectInput
            value={options.audioBitrate ?? "128k"}
            onChange={(e) => setOptions({ audioBitrate: e.target.value })}
          >
            {AUDIO_BITRATES.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </SelectInput>
        </FieldGroup>
      )}
    </div>
  );
}

/* Shared sub-components for all control panels */

export function SectionLabel({ children }: { children: React.ReactNode }) {
  return (
    <h3
      className="text-[11px] font-semibold uppercase tracking-widest"
      style={{ color: "var(--text-muted)" }}
    >
      {children}
    </h3>
  );
}

export function FieldGroup({
  label,
  trailing,
  children,
}: {
  label: string;
  trailing?: React.ReactNode;
  children: React.ReactNode;
}) {
  return (
    <div>
      <div className="mb-1.5 flex items-center justify-between">
        <label className="text-xs font-medium" style={{ color: "var(--text-secondary)" }}>
          {label}
        </label>
        {trailing && (
          <span style={{ color: "var(--text-muted)" }}>{trailing}</span>
        )}
      </div>
      {children}
    </div>
  );
}

export function ChipButton({
  active,
  children,
  onClick,
  className = "",
}: {
  active: boolean;
  children: React.ReactNode;
  onClick: () => void;
  className?: string;
}) {
  return (
    <button
      onClick={onClick}
      className={`px-2 py-1.5 text-center text-xs font-medium transition-all duration-100 ${className}`}
      style={{
        border: `1px solid ${active ? "var(--accent)" : "var(--border)"}`,
        backgroundColor: active ? "var(--accent-glow)" : "transparent",
        color: active ? "var(--accent)" : "var(--text-secondary)",
      }}
    >
      {children}
    </button>
  );
}

export function SelectInput({
  children,
  ...props
}: React.SelectHTMLAttributes<HTMLSelectElement>) {
  return (
    <select
      className="w-full border px-2 py-1.5 text-[13px]"
      style={{
        borderColor: "var(--border)",
        backgroundColor: "var(--bg-primary)",
        color: "var(--text-primary)",
      }}
      {...props}
    >
      {children}
    </select>
  );
}
