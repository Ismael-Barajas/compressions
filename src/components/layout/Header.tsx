import { useState, useRef, useEffect, type RefObject } from "react";
import { Moon, Sun, Clock, Terminal, Download, RefreshCw, Info } from "lucide-react";
import { useCompressionStore } from "../../stores/compressionStore";
import { useHistoryStore } from "../../stores/historyStore";
import { useLogStore } from "../../stores/logStore";
import { useUpdateCheck } from "../../hooks/useUpdateCheck";

// The header mark is the app icon's two duck feet (src-tauri/icons/icon.svg). Hovering it
// wakes the duck, which then waddles after the pointer along the header, leaving fading
// footprints, and walks back to stand in place when the pointer leaves the header.
const FOOT = "M240 360 L140 180 Q192 208 240 136 Q288 208 340 180 Z";
const FOOT_CENTER_X = 240;
const FOOT_CENTER_Y = 250;
// The icon's placement of each foot, as an SVG matrix(a b c d e f).
const LEFT_FOOT = [0.533663, -0.133057, 0.133057, 0.533663, -1.07719, 209.585];
const RIGHT_FOOT = [0.533663, 0.133057, -0.133057, 0.533663, 224.919, 5.71797];
// The mark shows the icon's 312-unit crop around (240, 240) at 18px.
const MARK_PX = 18;
const PX_PER_UNIT = MARK_PX / 312;

function footCenter(m: number[]): [number, number] {
  return [
    m[0] * FOOT_CENTER_X + m[2] * FOOT_CENTER_Y + m[4],
    m[1] * FOOT_CENTER_X + m[3] * FOOT_CENTER_Y + m[5],
  ];
}

// The logo is a duck facing up mid-stride. These measurements of it, in px, drive the
// walk, so the standing pose matches the icon exactly.
const [LEFT_X, LEFT_Y] = footCenter(LEFT_FOOT);
const [RIGHT_X, RIGHT_Y] = footCenter(RIGHT_FOOT);
const BODY_X = ((LEFT_X + RIGHT_X) / 2 - 240) * PX_PER_UNIT;
const BODY_Y = ((LEFT_Y + RIGHT_Y) / 2 - 240) * PX_PER_UNIT;
const HALF_WIDTH = ((RIGHT_X - LEFT_X) / 2) * PX_PER_UNIT;
const STRIDE = (LEFT_Y - RIGHT_Y) * PX_PER_UNIT;
const SPLAY_DEG = (Math.atan2(RIGHT_FOOT[1], RIGHT_FOOT[0]) * 180) / Math.PI;
const FOOT_SCALE = Math.hypot(RIGHT_FOOT[0], RIGHT_FOOT[1]) * PX_PER_UNIT;

// Walking. Speeds in px/s, turns in degrees/s.
const MAX_SPEED = 60;
const ACCEL = 140; // speeding up
const DECEL = 90; // slowing to a stop, gentler than ACCEL so it never overshoots
const TURN_GAIN = 5; // turn rate per degree still to turn, so turns ease in and out
const MAX_TURN = 260;
const START_PX = 6; // following: the pointer must be this far away before it sets off
const ARRIVE_PX = 0.5;
const STOP_GAP_PX = 20; // how far short of the header buttons it stops

// Wandering up and down while it walks, so the path isn't a straight line.
const WANDER_PX = 6; // most it strays from the header's centre line
const WANDER_FADE_PX = 40; // near home it walks straight, so it arrives on the line
const LOOK_AHEAD_PX = 32; // steering aims this far ahead, keeping the wander gentle
const Y_LIMIT_PX = 10;

// The look of the gait, scaled by how hard it is walking (0 standing, 1 full speed).
const ROCK_DEG = 4; // side-to-side lean each step: the waddle
const LIFT = 0.12; // a lifted foot draws this much bigger
const STRIDE_GROWTH = 0.6; // full-speed stride is this much longer than the standing one
const SETTLE_STEPS_PER_S = 3; // finishing the last step once stopped
const ENERGY_RATE = 4; // per second; eases the gait in and out

// Footprints left where each foot lands.
const PRINT_POOL = 10;
const PRINT_OPACITY = 0.35;
const PRINT_FADE_S = 1.4;

const DEG = Math.PI / 180;

// x, y: px from home (y down). heading: degrees, 0 = facing right, -90 = facing up.
// gait: steps taken; each whole step one foot swings past the other.
type Walk = { x: number; y: number; heading: number; gait: number; energy: number };
// Facing up, on a step where the right foot leads, feet together: the icon's pose.
const STANDING: Walk = { x: 0, y: 0, heading: -90, gait: 1, energy: 0 };

function footTransform({ x, y, heading, gait, energy }: Walk, isLeft: boolean): string {
  const step = Math.floor(gait);
  const f = gait - step;
  const swingIsLeft = step % 2 === 1;
  const swinging = isLeft === swingIsLeft;
  const eased = f * f * (3 - 2 * f);
  const rock = ROCK_DEG * energy * Math.sin(Math.PI * f) * (swingIsLeft ? 1 : -1);
  const facing = heading + rock;
  const rad = facing * DEG;
  const forwardX = Math.cos(rad);
  const forwardY = Math.sin(rad);
  const leftX = Math.sin(rad);
  const leftY = -Math.cos(rad);
  // The planted foot stays put on the ground while the body passes over it; the
  // swinging foot goes from half a stride behind to half a stride ahead.
  const stride = STRIDE * (1 + STRIDE_GROWTH * energy);
  const along = (swinging ? -0.5 - f + 2 * eased : 0.5 - f) * stride;
  const side = isLeft ? HALF_WIDTH : -HALF_WIDTH;
  const px = BODY_X + x + forwardX * along + leftX * side;
  const py = BODY_Y + y + forwardY * along + leftY * side;
  const angle = facing + 90 + (isLeft ? -SPLAY_DEG : SPLAY_DEG);
  const scale = FOOT_SCALE * (swinging ? 1 + LIFT * energy * Math.sin(Math.PI * f) : 1);
  return `translate(${px} ${py}) rotate(${angle}) scale(${scale}) translate(${-FOOT_CENTER_X} ${-FOOT_CENTER_Y})`;
}

const clampAbs = (value: number, limit: number) => Math.max(-limit, Math.min(limit, value));
const normalizeDeg = (deg: number) => ((((deg + 180) % 360) + 360) % 360) - 180;
// Shortest turn from one heading to another; a half turn goes via facing up.
const turnBetween = (from: number, to: number) => normalizeDeg(to - from);
// Eases value toward target at `rate` per second, landing exactly once close.
const ease = (value: number, target: number, rate: number, dt: number, snap: number) => {
  const next = value + (target - value) * (1 - Math.exp(-rate * dt));
  return Math.abs(target - next) < snap ? target : next;
};

function CompressionMark({
  headerRef,
  stopBeforeRef,
}: {
  headerRef: RefObject<HTMLElement | null>;
  stopBeforeRef: RefObject<HTMLDivElement | null>;
}) {
  const hitRef = useRef<HTMLSpanElement>(null);
  const homeRef = useRef<HTMLSpanElement>(null);
  const leftRef = useRef<SVGPathElement>(null);
  const rightRef = useRef<SVGPathElement>(null);
  const printRefs = useRef<(SVGPathElement | null)[]>([]);

  // Hovering the mark wakes the duck. While the pointer stays in the header the duck
  // follows it along, never past the header buttons; leaving the header sends it home.
  useEffect(() => {
    const header = headerRef.current;
    const hit = hitRef.current;
    if (!header || !hit) return;
    if (window.matchMedia?.("(prefers-reduced-motion: reduce)").matches) return;

    const w = { ...STANDING, speed: 0, travelled: 0 };
    let active = false;
    let walking = false;
    let pointerX = 0;
    // Each outing wanders along a different path.
    let wander = { k1: 0, k2: 0, p1: 0, p2: 0 };
    const printAges = Array<number>(PRINT_POOL).fill(Infinity);
    let nextPrint = 0;
    let frame = 0;
    let last = 0;

    const tick = (now: number) => {
      frame = 0;
      const dt = Math.min((now - last) / 1000, 0.05);
      last = now;
      const home = homeRef.current?.getBoundingClientRect();
      const stop = stopBeforeRef.current?.getBoundingClientRect();
      if (!home || !stop || dt <= 0) {
        frame = requestAnimationFrame(tick);
        return;
      }
      const homeX = home.left + home.width / 2;
      const farthestX = Math.max(0, stop.left - STOP_GAP_PX - homeX);
      const targetX = active ? Math.min(Math.max(pointerX - homeX, 0), farthestX) : 0;
      const dx = targetX - w.x;
      if (Math.abs(dx) > (active ? START_PX : ARRIVE_PX)) walking = true;
      else if (Math.abs(dx) < ARRIVE_PX) walking = false;
      const atHome = !active && !walking;

      // Steer toward the target, aiming a little up or down of it to wander.
      let wantHeading = atHome ? STANDING.heading : w.heading;
      let wantSpeed = 0;
      if (walking) {
        const fade = Math.min(1, Math.max(0, w.x / WANDER_FADE_PX));
        const wave =
          0.6 * Math.sin(w.travelled * wander.k1 + wander.p1) +
          0.4 * Math.sin(w.travelled * wander.k2 + wander.p2);
        const aheadX = Math.sign(dx) * Math.max(Math.abs(dx), LOOK_AHEAD_PX);
        wantHeading = Math.atan2(WANDER_PX * fade * wave - w.y, aheadX) / DEG;
        wantSpeed = Math.min(MAX_SPEED, Math.sqrt(2 * DECEL * Math.abs(dx)));
      }

      // Turn smoothly, and slow down while facing away from where it's going.
      const headingBefore = w.heading;
      const toTurn = turnBetween(w.heading, wantHeading);
      w.heading =
        Math.abs(toTurn) < 0.05
          ? wantHeading
          : normalizeDeg(w.heading + clampAbs(toTurn * TURN_GAIN, MAX_TURN) * dt);
      const turned = Math.abs(turnBetween(headingBefore, w.heading));
      const facing = Math.max(0, Math.cos(toTurn * DEG));
      const speedTarget = wantSpeed * facing * facing;
      const accel = speedTarget > w.speed ? ACCEL : DECEL;
      w.speed += clampAbs(speedTarget - w.speed, accel * dt);
      w.x += Math.cos(w.heading * DEG) * w.speed * dt;
      w.y = clampAbs(w.y + Math.sin(w.heading * DEG) * w.speed * dt, Y_LIMIT_PX);
      w.travelled += w.speed * dt;
      if (atHome) {
        w.x = ease(w.x, 0, 8, dt, 0.02);
        w.y = ease(w.y, 0, 8, dt, 0.02);
      }

      // Feet step with the distance walked, and shuffle round when turning on the spot.
      const stepBefore = Math.floor(w.gait);
      const stride = STRIDE * (1 + STRIDE_GROWTH * w.energy);
      w.gait += (w.speed * dt + turned * DEG * HALF_WIDTH) / stride;
      const effort = Math.min(1, w.speed / MAX_SPEED + turned / dt / MAX_TURN);
      w.energy = ease(w.energy, effort, ENERGY_RATE, dt, 0.005);

      // Once stopped, finish the step in progress. Back home, finish on a
      // right-foot-leading step so the feet land in the icon's pose.
      let settled = false;
      if (!walking && w.speed === 0 && w.heading === wantHeading) {
        const nextStep = Math.ceil(w.gait);
        const restAt = atHome && nextStep % 2 === 0 ? nextStep + 1 : nextStep;
        w.gait = Math.min(w.gait + SETTLE_STEPS_PER_S * dt, restAt);
        settled = w.gait === restAt && w.energy === 0 && (!atHome || (w.x === 0 && w.y === 0));
      }

      // A foot just landed: leave a print there.
      const stepNow = Math.floor(w.gait);
      if (stepNow > stepBefore && w.energy > 0.15) {
        printRefs.current[nextPrint]?.setAttribute("transform", footTransform(w, stepNow % 2 === 0));
        printAges[nextPrint] = 0;
        nextPrint = (nextPrint + 1) % PRINT_POOL;
      }
      let printsShowing = false;
      printAges.forEach((age, i) => {
        if (age === Infinity) return;
        const left = 1 - (age + dt) / PRINT_FADE_S;
        printAges[i] = left > 0 ? age + dt : Infinity;
        printRefs.current[i]?.setAttribute("opacity", String(PRINT_OPACITY * Math.max(0, left)));
        printsShowing ||= left > 0;
      });

      leftRef.current?.setAttribute("transform", footTransform(w, true));
      rightRef.current?.setAttribute("transform", footTransform(w, false));
      if (!settled || printsShowing) frame = requestAnimationFrame(tick);
    };

    const wake = () => {
      if (frame) return;
      last = performance.now();
      frame = requestAnimationFrame(tick);
    };
    const onEnterMark = (e: PointerEvent) => {
      pointerX = e.clientX;
      if (!active) {
        active = true;
        wander = {
          k1: 1 / (30 + Math.random() * 20),
          k2: 1 / (10 + Math.random() * 6),
          p1: Math.random() * 2 * Math.PI,
          p2: Math.random() * 2 * Math.PI,
        };
      }
      wake();
    };
    const onMove = (e: PointerEvent) => {
      pointerX = e.clientX;
      if (active) wake();
    };
    const onLeave = () => {
      if (!active) return;
      active = false;
      wake();
    };
    hit.addEventListener("pointerenter", onEnterMark);
    header.addEventListener("pointermove", onMove);
    header.addEventListener("pointerleave", onLeave);
    window.addEventListener("blur", onLeave);
    return () => {
      hit.removeEventListener("pointerenter", onEnterMark);
      header.removeEventListener("pointermove", onMove);
      header.removeEventListener("pointerleave", onLeave);
      window.removeEventListener("blur", onLeave);
      cancelAnimationFrame(frame);
    };
  }, [headerRef, stopBeforeRef]);

  return (
    <span ref={hitRef} className="-m-1.5 inline-flex p-1.5">
      <span ref={homeRef} className="relative block h-[18px] w-[18px]">
        <svg
          className="pointer-events-none absolute inset-0 z-10 overflow-visible"
          width={MARK_PX}
          height={MARK_PX}
          viewBox={`${-MARK_PX / 2} ${-MARK_PX / 2} ${MARK_PX} ${MARK_PX}`}
          fill="var(--accent)"
          stroke="var(--accent)"
          strokeWidth="26"
          strokeLinejoin="round"
          aria-hidden="true"
        >
          {Array.from({ length: PRINT_POOL }, (_, i) => (
            <path
              key={i}
              ref={(el) => {
                printRefs.current[i] = el;
              }}
              d={FOOT}
              opacity={0}
            />
          ))}
          <path ref={leftRef} d={FOOT} transform={footTransform(STANDING, true)} />
          <path ref={rightRef} d={FOOT} transform={footTransform(STANDING, false)} />
        </svg>
      </span>
    </span>
  );
}

export function Header() {
  const theme = useCompressionStore((s) => s.theme);
  const toggleTheme = useCompressionStore((s) => s.toggleTheme);
  const openHistory = useHistoryStore((s) => s.open);
  const openLogs = useLogStore((s) => s.open);
  const headerRef = useRef<HTMLElement>(null);
  const actionsRef = useRef<HTMLDivElement>(null);

  return (
    <header
      ref={headerRef}
      className="flex h-11 items-center justify-between border-b px-4"
      style={{ borderColor: "var(--border)", backgroundColor: "var(--bg-primary)" }}
      data-tauri-drag-region
    >
      <div className="flex items-center gap-2.5">
        <CompressionMark headerRef={headerRef} stopBeforeRef={actionsRef} />
        <h1
          className="text-[13px] font-semibold tracking-tight"
          style={{ color: "var(--text-primary)" }}
        >
          Compressions
        </h1>
      </div>
      <div ref={actionsRef} className="flex items-center gap-0.5">
        <UpdateButton />
        <HeaderButton onClick={openLogs} title="Application logs" aria-label="Application logs">
          <Terminal size={15} />
        </HeaderButton>
        <HeaderButton onClick={openHistory} title="Compression history" aria-label="Compression history">
          <Clock size={15} />
        </HeaderButton>
        <HeaderButton
          onClick={toggleTheme}
          title={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
          aria-label={`Switch to ${theme === "light" ? "dark" : "light"} mode`}
        >
          {theme === "light" ? <Moon size={15} /> : <Sun size={15} />}
        </HeaderButton>
      </div>
    </header>
  );
}

function UpdateButton() {
  const { updateAvailable, updateVersion, checking, checkForUpdate, installUpdate } =
    useUpdateCheck(false);
  const [popoverOpen, setPopoverOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);

  // Close popover on outside click
  useEffect(() => {
    if (!popoverOpen) return;
    const handleClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) {
        setPopoverOpen(false);
      }
    };
    document.addEventListener("mousedown", handleClick);
    return () => document.removeEventListener("mousedown", handleClick);
  }, [popoverOpen]);

  return (
    <div ref={ref} className="relative">
      <HeaderButton
        onClick={() => {
          if (!popoverOpen && !updateAvailable && !checking) {
            checkForUpdate();
          }
          setPopoverOpen((v) => !v);
        }}
        title={updateAvailable ? `Update to v${updateVersion}` : "Check for updates"}
        aria-label={updateAvailable ? `Update to v${updateVersion}` : "Check for updates"}
      >
        <div className="relative">
          <Info size={15} />
          {updateAvailable && (
            <span
              className="absolute -right-0.5 -top-0.5 h-2 w-2 rounded-full"
              style={{ backgroundColor: "var(--accent)" }}
            />
          )}
        </div>
      </HeaderButton>

      {popoverOpen && (
        <div
          className="absolute right-0 top-full z-50 mt-1 w-52 rounded-lg border p-3 shadow-lg"
          style={{
            backgroundColor: "var(--bg-secondary)",
            borderColor: "var(--border)",
          }}
        >
          {checking ? (
            <div className="flex items-center gap-2">
              <RefreshCw
                size={13}
                className="animate-spin"
                style={{ color: "var(--text-muted)" }}
              />
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                Checking for updates...
              </span>
            </div>
          ) : updateAvailable ? (
            <div className="flex flex-col gap-2">
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                v{updateVersion} available
              </span>
              <button
                onClick={() => {
                  setPopoverOpen(false);
                  installUpdate();
                }}
                className="flex items-center justify-center gap-1.5 rounded-md px-3 py-1.5 text-[12px] font-medium transition-opacity hover:opacity-90"
                style={{ backgroundColor: "var(--accent)", color: "white" }}
              >
                <Download size={13} />
                Install update
              </button>
            </div>
          ) : (
            <div className="flex flex-col gap-2">
              <span className="text-[12px]" style={{ color: "var(--text-secondary)" }}>
                Compressions v{__APP_VERSION__}
              </span>
              <button
                onClick={checkForUpdate}
                className="flex items-center justify-center gap-1.5 rounded-md border px-3 py-1.5 text-[12px] font-medium transition-colors"
                style={{
                  borderColor: "var(--border)",
                  color: "var(--text-secondary)",
                }}
                onMouseEnter={(e) => {
                  e.currentTarget.style.color = "var(--text-primary)";
                }}
                onMouseLeave={(e) => {
                  e.currentTarget.style.color = "var(--text-secondary)";
                }}
              >
                <RefreshCw size={13} />
                Check for updates
              </button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

function HeaderButton({
  children,
  ...props
}: React.ButtonHTMLAttributes<HTMLButtonElement>) {
  return (
    <button
      className="p-1.5 transition-colors duration-100"
      style={{ color: "var(--text-muted)" }}
      onMouseEnter={(e) => {
        e.currentTarget.style.color = "var(--text-primary)";
      }}
      onMouseLeave={(e) => {
        e.currentTarget.style.color = "var(--text-muted)";
      }}
      {...props}
    >
      {children}
    </button>
  );
}
