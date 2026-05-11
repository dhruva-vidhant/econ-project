import { useId } from "react";

interface Props {
  values: number[];
  width?: number;
  height?: number;
}

/** Tiny inline-SVG sparkline. No deps; sufficient for V1 SummaryWidgets. */
export default function Sparkline({ values, width = 120, height = 32 }: Props) {
  const id = useId();
  if (values.length < 2) {
    return <svg width={width} height={height} aria-hidden="true" />;
  }
  const min = Math.min(...values);
  const max = Math.max(...values);
  const span = max - min || 1;
  const stepX = width / (values.length - 1);
  const points = values
    .map((v, i) => `${i * stepX},${height - ((v - min) / span) * height}`)
    .join(" ");
  return (
    <svg width={width} height={height} aria-hidden="true">
      <polyline
        id={id}
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        points={points}
        className="text-accent"
      />
    </svg>
  );
}
