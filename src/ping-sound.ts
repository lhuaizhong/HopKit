export type PingSoundSettings = {ping_sound_enabled:boolean;ping_sound_when:string}
type PingEvent = {cursor?:number;event_type?:string;ok?:boolean;status?:string}

// Only tasks started in this window are tracked. Historical results never beep.
export class PingSound {
  private cursors = new Map<string,number>()
  private context:AudioContext|null = null
  private tone:OscillatorNode|null = null
  private lastTone = -Infinity

  constructor(private settings:()=>PingSoundSettings, private unavailable:()=>void) {}

  track(id:string) { this.cursors.set(id,0) }
  cursor(id:string) { return this.cursors.get(id) }
  forget(id:string) { this.cursors.delete(id) }
  stopAll() { this.cursors.clear(); this.silence() }

  async prepare() {
    this.context ??= new AudioContext()
    if(this.context.state==='suspended') await this.context.resume()
    if(this.context.state!=='running') throw Error('Audio unavailable')
  }

  consume(id:string, events:PingEvent[]) {
    let cursor = this.cursors.get(id)
    if(cursor===undefined) return
    const settings = this.settings()
    let matched = false
    for(const event of events) {
      const next = event.cursor
      if(typeof next!=='number' || !Number.isSafeInteger(next) || next<=cursor) continue
      cursor = next
      if(event.event_type==='sample' && event.status!=='cancelled' && event.status!=='interrupted'
          && event.ok===(settings.ping_sound_when==='success')) matched = true
    }
    this.cursors.set(id,cursor)
    // Consume muted events too, and coalesce a batch into at most one short tone.
    if(settings.ping_sound_enabled && matched) this.play()
  }

  play(preview=false) {
    if(!preview && performance.now()-this.lastTone<1000) return
    const context = this.context
    if(!context || context.state!=='running') { this.unavailable(); return }
    try {
      this.silence()
      const tone = context.createOscillator(), gain = context.createGain()
      const start = context.currentTime
      tone.type = 'sine'
      tone.frequency.setValueAtTime(this.settings().ping_sound_when==='success'?880:330,start)
      gain.gain.setValueAtTime(0,start)
      gain.gain.linearRampToValueAtTime(0.15,start+0.01)
      gain.gain.exponentialRampToValueAtTime(0.001,start+0.16)
      tone.connect(gain); gain.connect(context.destination)
      tone.onended=()=>{tone.disconnect();gain.disconnect();if(this.tone===tone)this.tone=null}
      this.tone=tone
      tone.start(start); tone.stop(start+0.18)
      this.lastTone=performance.now()
    } catch { this.unavailable() }
  }

  silence() {
    if(this.tone) { try { this.tone.stop() } catch {} this.tone=null }
  }

  dispose() {
    this.stopAll()
    void this.context?.close().catch(()=>{})
    this.context=null
  }
}
