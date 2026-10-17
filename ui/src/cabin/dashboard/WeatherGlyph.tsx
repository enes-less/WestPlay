import type { WeatherSymbol } from './weather'

/** The weather's symbol: WestPlay's own drawing, in the colours of the sky (a sun, a moon, a rain cloud). */
export function WeatherGlyph({ symbol, size }: { symbol: WeatherSymbol; size: number }) {
  return (
    <svg className={`weather-glyph ${symbol}`} width={size} height={size} viewBox="0 0 64 64" aria-hidden>
      {symbol === 'sun' && (
        <g>
          <circle cx="32" cy="32" r="12" fill="#ffcc00" />
          {Array.from({ length: 8 }, (_, i) => (
            <line
              key={i}
              x1="32"
              y1="9"
              x2="32"
              y2="15"
              stroke="#ffcc00"
              strokeWidth="4"
              strokeLinecap="round"
              transform={`rotate(${i * 45} 32 32)`}
            />
          ))}
        </g>
      )}
      {symbol === 'moon' && <path d="M40 10a22 22 0 1 0 14 34A18 18 0 0 1 40 10z" fill="#f5f3e6" />}
      {symbol === 'rain' && (
        <g>
          <path d="M20 40a10 10 0 0 1-1-20 14 14 0 0 1 27-3 10 10 0 0 1 1 23z" fill="currentColor" opacity="0.92" />
          {[22, 32, 42].map((x) => (
            <line key={x} x1={x} y1="46" x2={x - 3} y2="55" stroke="#4aa3ff" strokeWidth="4" strokeLinecap="round" />
          ))}
        </g>
      )}
    </svg>
  )
}
