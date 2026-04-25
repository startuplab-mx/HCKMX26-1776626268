import { T } from "../theme";

interface Props {
  data: number[];
  labels: string[];
}

export function WeekChart({ data, labels }: Props) {
  const max = Math.max(...data);
  const min = Math.min(...data);
  const W = 300;
  const H = 80;
  const pts = data.map((v, i) => {
    const x = 20 + (i / (data.length - 1)) * (W - 40);
    const y = H - 10 - ((v - min) / (max - min || 1)) * (H - 20);
    return [x, y] as [number, number];
  });
  const path = pts.map((p, i) => `${i === 0 ? "M" : "L"}${p[0]},${p[1]}`).join(" ");
  const area = `${path} L${pts[pts.length - 1][0]},${H} L${pts[0][0]},${H} Z`;
  return (
    <svg width="100%" viewBox={`0 0 ${W} ${H}`} preserveAspectRatio="xMidYMid meet">
      <defs>
        <linearGradient id="areaGrad" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor={T.primary} stopOpacity="0.3" />
          <stop offset="100%" stopColor={T.primary} stopOpacity="0" />
        </linearGradient>
      </defs>
      <path d={area} fill="url(#areaGrad)" />
      <path d={path} fill="none" stroke={T.primary} strokeWidth="2" strokeLinejoin="round" />
      {pts.map(([x, y], i) => (
        <circle key={i} cx={x} cy={y} r="3" fill={T.primary} />
      ))}
      {labels.map((l, i) => (
        <text
          key={i}
          x={pts[i][0]}
          y={H - 1}
          textAnchor="middle"
          fontSize="9"
          fill={T.text2}
          fontFamily="Space Grotesk"
        >
          {l}
        </text>
      ))}
    </svg>
  );
}
