import { useState } from "react";
import { useCompressionStore } from "../../stores/compressionStore";
import type { AudioCompressionFormat } from "../../types/compression";
import { SectionLabel, FieldGroup, ChipButton, SelectInput } from "./VideoControls";

const FORMATS: { value: AudioCompressionFormat; label: string }[] = [
  { value: "Original", label: "Original" },
  { value: "Mp3", label: "MP3" },
  { value: "Aac", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Flac", label: "FLAC" },
  { value: "Wav", label: "WAV" },
];

const BITRATE_PRESETS = ["64k", "96k", "128k", "192k", "256k", "320k"];

const SAMPLE_RATES = [
  { label: "Original", value: null },
  { label: "96000 Hz", value: 96000 },
  { label: "48000 Hz", value: 48000 },
  { label: "44100 Hz", value: 44100 },
  { label: "22050 Hz", value: 22050 },
];

export function AudioCompressionControls() {
  const options = useCompressionStore((s) => s.audioCompressionOptions);
  const setOptions = useCompressionStore((s) => s.setAudioCompressionOptions);
  const [customBitrate, setCustomBitrate] = useState("");

  const isExplicitLossless = options.format === "Flac" || options.format === "Wav";
  const showBitrate = !isExplicitLossless;

  return (
    <div className="space-y-5">
      <SectionLabel>Audio Compression</SectionLabel>

      {/* Format */}
      <FieldGroup label="Output Format">
        <div className="grid grid-cols-3 gap-1.5">
          {FORMATS.map((f) => (
            <ChipButton
              key={f.value}
              active={options.format === f.value}
              onClick={() => setOptions({ format: f.value })}
            >
              {f.label}
            </ChipButton>
          ))}
        </div>
        {options.format === "Original" && (
          <p className="text-[11px] mt-1.5" style={{ color: "var(--text-muted)" }}>
            Formats without a matching encoder (e.g. APE, DTS, ALAC) will be saved as MP3.
          </p>
        )}
      </FieldGroup>

      {/* Bitrate */}
      {showBitrate && (
        <FieldGroup label="Bitrate">
          <div className="grid grid-cols-3 gap-1.5">
            {BITRATE_PRESETS.map((b) => (
              <ChipButton
                key={b}
                active={options.bitrate === b}
                onClick={() => {
                  setOptions({ bitrate: b });
                  setCustomBitrate("");
                }}
              >
                {b}
              </ChipButton>
            ))}
          </div>
          <div className="mt-2 flex items-center gap-2">
            <input
              type="text"
              placeholder="Custom (e.g. 160k)"
              value={customBitrate}
              onChange={(e) => {
                setCustomBitrate(e.target.value);
                if (e.target.value) {
                  setOptions({ bitrate: e.target.value });
                }
              }}
              className="w-full border px-2 py-1.5 text-[13px]"
              style={{
                borderColor: "var(--border)",
                backgroundColor: "var(--bg-primary)",
                color: "var(--text-primary)",
              }}
            />
          </div>
        </FieldGroup>
      )}

      {/* Sample Rate */}
      <FieldGroup label="Sample Rate">
        <SelectInput
          value={options.sampleRate ?? "original"}
          onChange={(e) =>
            setOptions({
              sampleRate: e.target.value === "original" ? null : Number(e.target.value),
            })
          }
        >
          {SAMPLE_RATES.map((s) => (
            <option key={s.label} value={s.value ?? "original"}>
              {s.label}
            </option>
          ))}
        </SelectInput>
      </FieldGroup>
    </div>
  );
}
