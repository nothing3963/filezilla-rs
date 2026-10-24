export function Logo({ size = 20, className }: { size?: number; className?: string }) {
  return (
    <svg
      width={size}
      height={size}
      viewBox="0 0 1024 1024"
      fill="none"
      className={className}
      aria-hidden="true"
    >
      <defs>
        <linearGradient id="fz-logo-bg" x1="128" y1="96" x2="896" y2="928" gradientUnits="userSpaceOnUse">
          <stop stopColor="#22D3EE" />
          <stop offset="0.5" stopColor="#0EA5E9" />
          <stop offset="1" stopColor="#2563EB" />
        </linearGradient>
      </defs>
      <rect x="48" y="48" width="928" height="928" rx="224" fill="url(#fz-logo-bg)" />
      <rect x="49" y="49" width="926" height="926" rx="223" stroke="#fff" strokeOpacity="0.24" strokeWidth="2" />
      <g fill="#fff">
        <rect x="292" y="330" width="88" height="376" rx="44" />
        <path d="M336 236c16.8 0 33 6.6 45 18.4l126 122.7c25.2 24.5 25.8 64.8 1.3 90-24.5 25.2-64.8 25.8-90 1.3L384 438v268c0 35.3-28.7 64-64 64s-64-28.7-64-64V438l-34.3 30.4c-25.2 24.5-65.5 23.9-90-1.3-24.5-25.2-23.9-65.5 1.3-90l126-122.7c12-11.8 28.2-18.4 45-18.4Z" />
        <rect x="644" y="318" width="88" height="376" rx="44" />
        <path d="M688 800c-16.8 0-33-6.6-45-18.4L517 658.9c-25.2-24.5-25.8-64.8-1.3-90 24.5-25.2 64.8-25.8 90-1.3l34.3 30.4V330c0-35.3 28.7-64 64-64s64 28.7 64 64v268l34.3-30.4c25.2-24.5 65.5-23.9 90 1.3 24.5 25.2 23.9 65.5-1.3 90L766 781.6c-12 11.8-28.2 18.4-45 18.4Z" />
      </g>
    </svg>
  );
}
