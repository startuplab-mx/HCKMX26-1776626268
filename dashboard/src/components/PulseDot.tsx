interface Props {
  color: string;
  size?: number;
}

export function PulseDot({ color, size = 10 }: Props) {
  return (
    <span
      style={{
        position: "relative",
        display: "inline-flex",
        alignItems: "center",
        justifyContent: "center",
        width: size,
        height: size,
      }}
    >
      <span
        style={{
          position: "absolute",
          width: "100%",
          height: "100%",
          borderRadius: "50%",
          background: color,
          opacity: 0.4,
          animation: "ping 1.5s cubic-bezier(0,0,0.2,1) infinite",
        }}
      />
      <span
        style={{
          width: size * 0.6,
          height: size * 0.6,
          borderRadius: "50%",
          background: color,
        }}
      />
    </span>
  );
}
