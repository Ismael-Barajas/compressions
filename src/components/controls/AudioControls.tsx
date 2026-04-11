import { useCompressionStore } from "../../stores/compressionStore";
import type { AudioOutputFormat } from "../../types/compression";
import { SectionLabel, FieldGroup, ChipButton, SelectInput } from "./VideoControls";

const FORMATS: { value: AudioOutputFormat; label: string }[] = [
  { value: "Mp3", label: "MP3" },
  { value: "Aac", label: "AAC" },
  { value: "Opus", label: "Opus" },
  { value: "Flac", label: "FLAC" },
  { value: "Wav", label: "WAV" },
];

const BITRATES = ["64k", "96k", "128k", "192k", "256k", "320k"];

const SAMPLE_RATES = [
  { label: "Original", value: null },
  { label: "48000 Hz", value: 48000 },
  { label: "44100 Hz", value: 44100 },
  { label: "22050 Hz", value: 22050 },
];

export function AudioControls() {
  const options = useCompressionStore((s) => s.audioOptions);
  const setOptions = useCompressionStore((s) => s.setAudioOptions);

  const isLossless = options.format === "Flac" || options.format === "Wav";

  return (
    <div className="space-y-5">
      <SectionLabel>Audio Extraction</SectionLabel>

      {/* Format */}
      <FieldGroup label="Format">
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
      </FieldGroup>

      {/* Bitrate */}
      {!isLossless && (
        <FieldGroup label="Bitrate">
          <SelectInput
            value={options.bitrate ?? "192k"}
            onChange={(e) => setOptions({ bitrate: e.target.value })}
          >
            {BITRATES.map((b) => (
              <option key={b} value={b}>
                {b}
              </option>
            ))}
          </SelectInput>
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
