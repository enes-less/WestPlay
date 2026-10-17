import { type CSSProperties, type ReactNode, type RefObject, useEffect, useState } from 'react'

/** Where the screen stands: centre stage (`hero`, `final`), aside for the steps (`side`), the player
 * diving into it at the end (`dive`), turned aside beside the companion's pages (`aside`), or full size, to
 * be used (`full`, the tutorial). */
export type Pose = 'hero' | 'side' | 'final' | 'dive' | 'aside' | 'full'

/** A CarPlay-style banner on the screen: WestPlay telling the player what it's doing. */
export interface Banner {
  title: string
  detail?: string
  /** 0 to 1: a bar under it. */
  progress?: number
  busy?: boolean
}

/**
 * The truck's screen, as an object: a slab of glass in its bezel, standing in a dark room it lights, its
 * reflection on the floor, turning a little towards the player's mouse. On it, the live WestPlay, and over
 * its glass whatever the step adds: a banner, the light of a scan, a cursor.
 */
export function TruckScreen({
  frame,
  ready,
  pose,
  glow,
  banner,
  scanning,
  children,
}: {
  frame: RefObject<HTMLIFrameElement | null>
  /** WestPlay in it has loaded: it fades in. */
  ready: boolean
  pose: Pose
  /** The colour the screen's light casts on the room. */
  glow: string
  banner: Banner | null
  scanning?: boolean
  /** More over the glass, in the screen's own pixels. */
  children?: ReactNode
}) {
  return (
    <div className="ts" data-pose={pose} style={{ '--glow': glow } as CSSProperties}>
      <div className="ts-light" />
      <div className="ts-tilt">
        <div className="ts-bezel">
          <div className="ts-glass">
            <div className="ts-frame">
              <iframe
                ref={frame}
                src="/cabin.html?demo&power"
                title="WestPlay"
                tabIndex={-1}
                data-ready={ready || undefined}
              />
              {scanning && <div className="ts-scan" />}
              <BannerView banner={banner} />
              {children}
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}

/** How long a banner's old words take to leave when new ones come (onboarding.css `ts-words-out`). */
const WORDS_OUT_MS = 420

/**
 * The banner, as iOS's: it drops in from the top on a spring and slides back up when it goes. A new message
 * (another title) makes it give a little, its icon pop, and the old words lift away as the new ones rise in;
 * the same message updating (the map's percentage) only changes its numbers.
 */
function BannerView({ banner }: { banner: Banner | null }) {
  // The last banner, kept while it slides away; the words it had before a new message, while they leave;
  // and how many messages it has had, each change of message restarting its little give.
  const [shown, setShown] = useState(banner)
  const [leaving, setLeaving] = useState<Banner | null>(null)
  const [messages, setMessages] = useState(0)
  if (banner && banner !== shown) {
    if (shown && banner.title !== shown.title) {
      setLeaving(shown)
      setMessages((count) => count + 1)
    }
    setShown(banner)
  }
  useEffect(() => {
    if (!leaving) return
    const timer = window.setTimeout(() => setLeaving(null), WORDS_OUT_MS)
    return () => window.clearTimeout(timer)
  }, [leaving])

  const text = banner ?? shown
  return (
    <div
      className="ts-banner"
      data-open={banner ? '' : undefined}
      // Two names for the same give, taking turns: a new name starts it again.
      data-give={messages ? (messages % 2 ? 'a' : 'b') : undefined}
    >
      {text && (
        <>
          <img src="/westplay.png" alt="" />
          <div className="ts-banner-words">
            {leaving && <BannerWords banner={leaving} leaving />}
            <BannerWords key={text.title} banner={text} entering={messages > 0} />
          </div>
          {text.busy && <span className="ts-busy" />}
        </>
      )}
    </div>
  )
}

function BannerWords({ banner, leaving, entering }: { banner: Banner; leaving?: boolean; entering?: boolean }) {
  return (
    <div
      className="ts-banner-text"
      data-leaving={leaving || undefined}
      data-entering={(entering && !leaving) || undefined}
      aria-hidden={leaving || undefined}
    >
      <b>{banner.title}</b>
      {banner.detail && <span>{banner.detail}</span>}
      {banner.progress !== undefined && (
        <i className="ts-banner-bar">
          <i style={{ width: `${Math.round(banner.progress * 100)}%` }} />
        </i>
      )}
    </div>
  )
}

/** A pointer over the glass, at `x`, `y` (the screen's pixels), pressing when `pressing`. */
export function GlassCursor({
  x,
  y,
  pressing,
  visible,
}: {
  x: number
  y: number
  pressing: boolean
  visible: boolean
}) {
  return (
    <div
      className="ts-cursor"
      data-pressing={pressing || undefined}
      data-visible={visible || undefined}
      style={{ transform: `translate(${x}px, ${y}px)` }}
    >
      <i />
    </div>
  )
}
