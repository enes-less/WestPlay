# Changelog

What changed in each release of WestPlay. The newest release's notes are what the app shows when it offers the
update.

## 2.1.4 (2026-10-28)
- The cursor on WestPlay's screen stays where you left it when you let go of the interaction key, and fades away
  five seconds after you last moved it, as it does when you stop moving it: it went the moment you let go.
- The tablet stays blank in a truck whose own screen shows WestPlay (a Scania S with the tablet fitted): with both
  showing, WestPlay was laid out for one screen and then the other on every frame, and slowed down until it froze.
- YouTube Music plays songs YouTube shows a warning before ("this video may contain disturbing images"), as
  YouTube already does: the song stood still at its start behind the warning, which had no one to press continue.
- YouTube Music gets past YouTube's "Before you continue" cookie page in the EU, the UK and Switzerland: it could
  stay on it, nothing loading. WestPlay answers it as Reject all does, in YouTube too.
- Over a light album cover, Now Playing's words go dark, in Apple Music and in the Dashboard's widget, the
  sidebar's too: they stayed white on the light colours.
- The cab's sidebar is almost a pill again, its corners round, with more room at its top and bottom so the clock
  stays clear of its round ends: 2.1.3 left it nearly square.
- Music keeps playing while the game shows, with another window in front of it or not: with Lossless Scaling
  (its window is in front of the game it scales) YouTube, YouTube Music, Spotify and Apple Music paused as if
  you had tabbed out. It pauses as the game is minimised, and with the game's menu, as before. WestPlay's
  keybinds (volume, play/pause) work there too: they went dead with another window in front.
- With the camera outside the truck, music no longer plays for a moment as if you were in the cab while you
  go between the game's menus: WestPlay took the cab, drawn again behind a menu, for the menu gone and the
  camera back inside. The music now waits for the game to run again.
- With the game run as administrator, WestPlay says so: the companion's Truck Screen page, and Maps when you
  start a route ("The game didn't answer" before). WestPlay can't see the game's route, guidance or weather then;
  start the game normally, not as administrator.

## 2.1.3 (2026-10-27)
- The WestPlay tablet has a new look: a slim aluminium tablet with a black glass front, at a real tablet's size
  (24.8 x 17.9 cm), WestPlay laid out in the shape of its screen. The tablet's model is Art_bor's (Sketchfab,
  CC BY 4.0).
- Hanging, the tablet is held by a clamp over its top edge, on a short, sturdy arm: the long thin bar it hung
  from is gone.
- On the tablet, WestPlay uses more of the screen: the margin round it is smaller, as the tablet shows all of
  WestPlay's picture.
- Every panel's corners are rounded the same, as the Dashboard's widgets are: the sidebar, the map, the apps'
  windows and the keyboard each had their own.
- The truck's screen no longer freezes on its last picture after the game runs again from its menu: in some
  players' games, WestPlay stopped seeing the screen being drawn and left it as it was until the next pause.
- YouTube's videos fill the player: a video could play small in its top left corner, most often the next one
  after the first.
- Maps takes less of your graphics card and processor, most of all in towns with their 3D buildings: it was
  drawn as often as your monitor refreshes (144 times a second or more on some), though the game shows at most 60
  of WestPlay's frames. It's drawn at most 60 times a second now.
- In a truck with the tablet, the companion says WestPlay is on the truck's screen: it kept saying it was waiting
  for the cab, and its switch for WestPlay's screen stayed greyed out.
- The companion's window holds still while it isn't in front: its floating screen kept drawing behind the game,
  taking up to two processor threads the game could use. It goes on the moment you click it.
- The cursor on WestPlay's screen follows the mouse at once: it was drawn by the screen itself and showed three or
  four frames late, more at a low frame rate. WestPlay now draws it straight into the frame the game shows, the
  same dot as before, in a few milliseconds.
- WestPlay's log says more when its screen is slow: every 10 seconds instead of every minute, the screen's own
  frame rate beside the frames Windows delivers, the companion window's share of the processor, and the game's
  window (fullscreen, borderless or a window, and when it's in the background).
- The companion says when YMN One, another CarPlay mod, is installed in the game: it takes over the truck's GPS
  screen, so WestPlay can't show there, and the screen turns pink while YMN One isn't running. It says which file
  to delete.
- YouTube videos no longer stop playing altogether with "This content isn't available, try again later": the way
  WestPlay asks for a video without the ad's wait sometimes got that answer, and was kept for every video after it.
  That way is now dropped for the video asked again YouTube's own way, so it plays.
- YouTube and YouTube Music work in the EU, the UK and Switzerland: YouTube's "Before you continue to YouTube"
  cookie page stood in for them, with no one to answer it. WestPlay answers it for you as Reject all does.
- YouTube plays videos it shows a warning before ("this video may contain disturbing images"), such as some songs'
  videos: WestPlay's player stayed on the warning, which had no one to press continue.
- Spotify shows all your Liked Songs, and every song of a long playlist or album: the list stopped at the first
  50 (Liked Songs and albums) or 100 (playlists). The rest now load as you scroll down, as in Spotify.

## 2.1.2 (2026-10-26)
- On TruckersMP, your own truck no longer shows on the map as another player's dot following you.

## 2.1.1 (2026-10-26)
- Setup no longer says WebView2 isn't working on PCs where it is: 2.1.0's check looked in the wrong place in the
  registry, so it stopped every install and update.

## 2.1.0 (2026-10-26)
- WestPlay's plugin loads in the game on PCs without Microsoft's Visual C++ Redistributable: the game said
  "unable to load" (0000007E) in its game.log, and there was no WestPlay on the truck's screen while the app ran.
- Spotify pauses with the game and plays on as you come back, fading in, every time: leaving the game and
  coming back quickly could leave the music stopped, and a song could start late.
- Pausing with an outside camera no longer plays the music clear for a moment, as if from inside the cab: it
  stays muffled as it fades out.
- Text Size has two smaller steps, and Home's app names and Settings' titles follow it; the sidebar keeps its
  size, as in CarPlay.
- When a YouTube video doesn't start within 20 seconds, the log says why, as YouTube's page tells it: whether YouTube
  lets it play, its error, and what the video player has received. For players whose videos never start.
- The companion says when your truck isn't one WestPlay shows in ("This truck isn't supported"), instead of
  waiting for the cab, and that fitting the WestPlay tablet brings WestPlay into any truck.
- Setup checks that Microsoft Edge WebView2 Runtime works on your PC before installing: without it WestPlay
  couldn't start at all (it closed at once, showing nothing). Setup now says so, offers Microsoft's page to
  install it from, and stops.
- WestPlay in any truck: the WestPlay tablet, which you fit in the truck shop (Accessories: Hanging Items, or as
  a dashboard set), shows WestPlay in trucks with no screen of its own, the truck's own screen keeping the game's
  navigation. Hanging, it's on a short bar from the top of the dashboard, upright and level in every truck; on a
  dashboard set, it faces you. It also stands on the toy spots of a dashboard set you've fitted, such as a table's. As with the trucks' own screens, the music sounds from outside the cab as it would through its
  walls. It's in every truck with hanging items, mod trucks too: a truck mod you install later gets it the
  next time you start the game. The tablet's model is Kedi739's, used with his permission.
- Maps are prepared again for players with certain mods: a small packed file in a mod could stop WestPlay from
  preparing any map at all, leaving the setup's map step waiting for ever and Prepare Maps Again gone. A map
  that can't be made now fails on its own, with a reason, and the other profiles' maps are still made.
- YouTube Music starts at the volume you last set, instead of at full volume every time.
- Spotify no longer plays in silence once its volume is down to nothing: the volume bar disappeared at 0 (and
  with it the button to unmute), and the volume you last set was never given back, so a song seemed to play
  with no sound. The bar now stays, at 0, and Spotify is set back to your volume when it starts.
- More map mods show on the map: ones made with an older map editor (such as Sud de France and Bourges
  Updated), and ones protected in a way WestPlay couldn't read (such as Crni Kal). WestPlay had left them out
  of your map; a map that left a mod out is prepared again by itself, others are kept as they are.
- WestPlay's log (app.log) is always kept now. Where Windows' ransomware protection (Controlled folder access)
  or an antivirus keeps WestPlay out of Documents, there was no log at all; it's now kept in WestPlay's own
  folder, saying why, and About shows where it is. Starting WestPlay again while it was running also emptied
  the log; it no longer does.
- WestPlay on the screens of Oyuncuyusbis' coaches: the Mercedes-Benz Travego 16 SHD, the MAN Lion's Coach and the
  Setra S 531 DT on each one's own dashboard screen, and the Setra S 516 HD/2 on its tablet (its "Gps" part, fitted
  in the truck shop), each shaped to its screen and drawn sharp. Install the coach's mod and WestPlay sets its
  screen up by itself.
- Music plays from the speakers of JuseeTV's Scania PGR, as fitted in the truck shop: its door speakers, the
  A-pillars' tweeters, the top console's centre speaker (or Dark Diamond's pair), and the rear walls with
  speakers (JS Customs, V8 Power, Dark Diamond, Lights + speakers). A rear wall's biggest units play the bass;
  its smaller ones, where it has them, a quiet fill a moment after the front, so the music stays in front of
  you and the cab sounds bigger. Parts you change count once the game saves.
- The Renault T's speakers as each interior has them: the two speakers on the back wall over the bed in every
  T, the A-pillars' tweeters, and the door speakers only in the 2024 interiors. In the older interiors the music
  comes from the back wall, as in the truck. The low sleeper's speakers are now where its cab has them, no longer
  a high sleeper's, and right-hand-drive Ts have them mirrored. A truck you've just bought or changed counts once
  the game saves.
- The MAN TGX (2020)'s speakers: a woofer low in each door, a smaller unit above it for the voices, and a tweeter
  on each A-pillar, in every cab (GM, GN, GX), right-hand drive too.
- WestPlay on the screens of the DAF XF Electric and XD (the XF's screen, with its DAF strip) and the Scania S
  BEV (the S's 7" screen, drawn sharp as the S's is).
- The DAF XD's speakers: the XF and XG's, where its cab has them. The XF Electric has the XF's, and the Scania S
  BEV the S's, and the Renault E-Tech T the T 2024's.
- The Mercedes-Benz Actros (2014)'s speakers: a woofer low in each door, a unit high over each door for the voices,
  and a tweeter on each A-pillar, in every cab. The Actros (2009)'s: a woofer low in each door and a unit at each
  front corner of the overhead shelf. The Scania R (2009) and Streamline's: a woofer in each door, a tweeter in
  each A-pillar. The Volvo FH16 (2012) has the FH's, and the FH16 (2009) a speaker in each door. The DAF XF (105)'s
  as each cab has them: A-pillar tweeters, and two pairs on the overhead shelf in the Super Space Cab or one
  overhead speaker in the Space Cab and Space Cab Plus. The DAF XF Euro 6's: door woofers and A-pillar tweeters, with
  two pairs on the overhead shelf in the Super Space Cab or one overhead speaker in the other cabs. The MAN TGX's:
  door woofers, and in the XXL two small speakers high in each side wall. The MAN TGX Euro 6's: door woofers, and in
  the XL a small speaker in each roof corner. The Renault Magnum's: a speaker in each roof corner, and tweeters under
  the overhead console. The Renault Premium's: a speaker high in each side of the roof, as each roof has it.
  The Iveco Stralis's: a speaker in each end of the overhead console, and in the high roofs a small one high on the
  passenger's side; the Iveco Hi-Way's: the console's two, and one high on each side.

## 2.0.3 (2026-10-23)
- Spotify plays on when you come back to the game: leaving it and coming back could leave Spotify playing in
  silence, or quieter each time, its volume stuck part way through the fade.

## 2.0.2 (2026-10-23)
- WestPlay works on older versions of the game too, such as 1.58: 2.0.1 found what it needs in every version
  but there, where the game is built a little differently.
- The Companion's Map page shows its maps and its Prepare Maps Again button while WestPlay removes an old map
  no profile plays any more: removing a big one takes minutes, and until it was done the page had nothing to
  show and the button was gone.

## 2.0.1 (2026-10-23)
- WestPlay works on other versions of the game, not only the current Steam one: it now finds what it needs in
  the game itself, whatever its version. The VR version and older versions such as 1.60 included, and it should
  keep working through game updates.

## 2.0.0 (2026-10-22)
- TruckersMP: WestPlay works there, its screens brought into the game by its plugin as in single player.
  Maps uses TruckersMP's own map (its HQ and event cities), prepared as soon as TruckersMP is found.
- TruckersMP: the other players show on the map, live around you and further out every few seconds, and
  traffic on your route shows on the blue line in yellow and red where trucks are bunched up, as you choose
  a route and as you drive.
- Maps at night in Apple Maps' own dark colours: slate-blue ground, lighter buildings, deep blue water, teal
  parks, and roads as light lines further out and dark bands closer in.
- Maps by day in Apple Maps' own light colours: cream ground, white streets, grey motorways, sky-blue water;
  the 3D buildings in the colour of the flat ones, their walls softly shaded rather than near black.
- Maps shows the buildings in 3D as you drive, in their own shapes and heights from the game's models (a
  tower's pod, a roof over the pumps) and in the colour of the flat ones, as Apple Maps does. A filling
  station's or a border crossing's roof is left out while you're under it. Every map is prepared again once.
- Filling stations, garages, services, hotels, recruitment agencies and company depots stand in 3D too, their
  own buildings from the game's models, rather than lying flat.
- Warehouses and malls the game builds from walls stand as whole buildings, and the details on their roofs no
  longer stand in rows on the ground.
- Filling stations on Maps: no more white blobs where a station stands beside the road, and the roads through
  stations on the road join on at their own width instead of stopping short.
- One-way service roads in the Balkans join their forks and stations smoothly on Maps, without a step at each join.
- Tunnels on Maps: roads through tunnels are drawn as a faint band, as Apple Maps draws them.
- Text Size in Settings › Appearance, as on iPhone: seven steps from smaller to larger, for the text in every
  app and the names on the map, the icons and artwork as they are.
- An equalizer in Settings › Sound, as on iPhone: Recommended or Custom, Low, Mid and High dragged up or down
  (up to 6 dB each way) for every player and every truck, with the song playing and its waveform moving under
  the curve as you set it.
- The Volvo FH's own speakers: music plays from the woofers low in its doors, with the bass from the panels on
  its back wall.
- With head tracking, the truck's speakers no longer crackle as the tracker's small wobbles turn the sound:
  it follows your head only as it really moves.
- A truck mod is no longer taken for a map mod, so it doesn't get a map of its own prepared for nothing.
- The Companion's Map page shows one progress number in its bar and its sidebar, never running ahead of the
  map being prepared.
- WestPlay removes the maps no profile plays any more (a profile deleted, its map mods changed), freeing their
  space.
- Handing a route to the game's GPS: WestPlay's hidden waypoints go only on real road pieces, never on a
  junction or a border crossing's lanes, and at border crossings and toll plazas the game picks the lane trucks
  use. Before, the screen could flash several times and the game end up on its own way.
- Maps offers routes again where it found none, such as on a toll plaza's approach.
- Routes never go along roads no vehicle may drive.
- With the game's Time zones option on (Only time or Full info), the clock shows the time where you are, as
  the game's clock does: it was an hour early in Finland, the Baltics, Romania, Bulgaria, Greece and Turkey,
  and an hour late in the UK and Portugal. With it off, the clock stays the game's time. Every map is prepared
  again once.
- Pausing the game (its menu, its map) gives the mouse back to the game, also with "Press once instead of
  holding" on: the menu couldn't be used. Back in the game, press WestPlay's key again to use the screen.
- A stop you add goes where it fits along the way, as the game's own map puts one: from A to F, adding D and
  then B gives A, B, D, F. Without a job, a stop no longer becomes the end of the route.
- With stops on the route, the arrival pill shows the time to the next stop beside the time to the end
  ("30 min | 3 h 5 min"); once you reach it, the time to the one after.
- Closing the game's menu, the music comes back as the game's picture does, not 1.5 seconds later when the
  truck moves on.
- "Press once instead of holding" works: once on, pressing WestPlay's key again didn't turn it off.
- Apple Music's lyrics no longer pile up: each song's words stayed on the screen under the next song's. Spotify's
  home no longer risks showing a shelf twice or leaving one out when several share a title.
- Apple Music's synced lyrics follow every song: on songs whose times Apple writes in seconds, the last line had
  always shown as the one sung.
- Back in the game, the music fades in as it starts playing again, instead of staying silent for two seconds
  first (Spotify, Apple Music and YouTube Music).
- Apple Music's lyrics follow the song again, and a song from your Library no longer shows "couldn't load" over
  its own lyrics part way through.
- In Maps, pressing Go no longer makes the search panel jump from side to side while the game takes the route.
- Desktop icons can be clicked again while WestPlay runs: an invisible window of its truck screen covered the top
  left of the main screen and took the clicks there.
- WestPlay in the Volvo FH (2021 and 2024, their DLCs): the real FH has CarPlay with its INFOHIGH package.
- On the Scania S and R's 7" screen, WestPlay shows whole and sharp: its bottom was cut off, everything was
  drawn too tall, and the game drew the screen at a quarter of its resolution. Its sidebar shows two recent
  apps; the Dashboard's arrival time goes to the top of its map, clear of the speed and zoom buttons, and its
  next turn fits its column; Spotify's shortcuts show two across, with their names whole.
- A search box's hint too long for it shows its start, not its end. In Spotify, nothing shows under the player
  bar at the foot.
- The Dashboard's map has zoom buttons, as Maps does. They fade away after 15 seconds without using the map,
  and come back as you point at it.
- Keybinds: the music's volume up and down (5% a press) and play/pause, from the game. By default, hold
  WestPlay's key and press 1 (volume down), 2 (play/pause) or 3 (volume up); any of them can be any key, alone or with WestPlay's key, in the
  Companion's Controls. The volume shows at the top of the screen as it changes, as CarPlay shows it. A key
  bound to WestPlay no longer does anything in the game while it's WestPlay's.
- WestPlay's key can be any key now, not only the four offered, and it can be pressed once to use the screen
  and again to stop, instead of held (Controls, "Press once instead of holding").
- Leaving the game (its pause menu, the map, another window), the music fades out over half a second instead
  of cutting off, and fades back in as you return. Pausing it yourself still stops it at once.
- Spotify starts at the volume you last set, instead of its own every time (YouTube, YouTube Music and Apple
  Music already did).
- After switching from a truck WestPlay supports to one it doesn't, holding WestPlay's key no longer takes
  the mouse from the game.
- Maps shows the rest areas made without a rest area's own building block, where the game offers sleep by the
  road: over a thousand more on the base map, about 1,800 more with ProMods, as the game's own map shows them.
  Every map is prepared again once.
- WestPlay finds your game's folder when the game is started with `-homedir` in its Steam launch options, so
  it reads your own profiles and their mods (ProMods and others) instead of an old folder in Documents.
- In Light mode, the other routes Go offers are drawn pale, as Apple's are, instead of staying dark.
- Maps has a compass right over its zoom buttons, as CarPlay's Maps does: its red mark and N turn with the
  map, always pointing north. Tap it and the map turns north up, seen from above, still following your
  truck; the arrow then brings the view from behind the truck back.
- At toll plazas, the route on the map goes through a booth trucks may use, as the game's GPS sends you (often
  the right-hand ones), instead of straight through a booth for cars only.
- One player at a time, as on a phone: starting music or a video in Apple Music, Spotify, YouTube Music or
  YouTube pauses whichever of them was playing, instead of both playing over each other.
- When WestPlay offers an update, each change in its notes shows whole, instead of being cut off after its
  first line.
- A wallpaper of your own that's a single picture works in Light and Dark mode alike. It had kept the screen
  (and its icons) in one look, dark or light by the picture's brightness, with no way to switch.
- A map mod WestPlay can't read no longer stops a profile's map: the map is made without it, and the Map page
  names it on the profile. Mods whose archives give wrong sizes for their files (a trick of protected mods,
  which the game reads anyway) are read in full.
- On a route, Maps' search and Find nearby offer only places along the rest of the route, nearest along it
  first, with how far along the route each is and what stopping there adds ("+5 min"): no more stops
  behind you. Adding one says what it added, in the game's own distance and time.
- Go offers the routes to a place first, as Apple Maps does: the fastest, the shortest, one on small roads or
  another way, each with its time and distance, drawn on the map. The one you pick is handed to the game's
  GPS, which follows it, held to it by as few waypoints as it needs (up to nine, in the middle of roads); if
  it won't follow, it takes its own way, with none of them left behind. Every map is prepared again once, for
  its routes. The routes show whole, beside the card; a route can be picked on the map too, by tapping it; Go
  sits in the picked route's row. After Go, the map is back on the truck at the zoom it had before. Another
  way no longer leaves a motorway at an interchange only to come straight back onto it.
- WestPlay's truck screens no longer need turning on in Mod Manager, for each profile: WestPlay's plugin
  brings them into the game itself, in every profile and in Convoy, and they're not in your mod list at all.
  The old WestPlay mod is taken out of the game's mod folder and out of your profiles when WestPlay updates
  (with the game closed); uninstalling WestPlay still takes everything out.
- On the Scania Smart Dash Advanced (12.9"), WestPlay is laid out for its own screen: everything is smaller
  than on the 10.1", so it no longer looks too large, with text a little larger than the 10.1"'s so Maps'
  small print stays easy to read. WestPlay's
  screens mod now carries its own copies of those interiors' definitions to tell the two apart. Its taller
  sidebar shows five recent apps.
- Apple Music, Spotify and YouTube Music open smoothly the first time after the screen comes on: their window
  grows out of the icon showing a saved copy of how the app first looks (its own loading screens, signed
  in), and the app takes its place once open, its content loading as usual.
- Closing the game (or the game crashing) stops the music: Apple Music, Spotify, YouTube Music and YouTube
  are closed with it, instead of playing on, unseen, through the truck's speakers with no game. The screen
  starts afresh with the next game.
- When WestPlay's app and its game plugin are of different versions (one updated, the other not yet), they
  no longer try to work together: they had misread each other, and typing could take the truck's driving
  keys.

## 1.2.0 (2026-10-19)
- With map mods (ProMods, Türkiye maps and others), the route follows the mod's own junctions and the
  Maps app gives its turns there, instead of only where the mod left the game's roads as they were.
- WestPlay's screen is drawn on the same graphics card as the game, not on whichever card Windows lists
  first (the slow built-in one on many laptops). If it picks the wrong one, choose another under Truck
  Screen › Graphics Card. About shows WestPlay's card and the game's.
- ProMods maps show all their junctions: about 16,000 were missing, leaving roads cut short (most visible
  with ProMods Maghreb). ProMods players' maps are prepared again once, after the update.
- Maps with ProMods' Middle East add-on can be prepared: one of its roads was taken for a road 116 km away,
  and preparing the map ran out of memory.
- Preparing a map takes about half the memory (ProMods: 4 GB down to 2.2 GB) and runs faster on PCs with
  more cores. A map that would need more memory than the PC can spare is stopped, and the Map page says why.
- A filling station two fuel companies stand by is always named after the nearer one.
- Map mods are layered on the map as the game layers them: the one higher in the Mod Manager wins where two
  change the same thing (WestPlay had them the other way round). Every map is prepared again once.
- The Maps app shows the map of the profile you've loaded, as soon as you load it. It had followed the profile
  with the newest save, so a profile not saved for a while showed another profile's map (and garages).
- After Prepare Maps Again, the Map page's progress bar is all the maps being prepared, each profile an equal
  share (ten profiles: 10% each), instead of saying it's done once yours is. Each profile's row shows its own.

## 1.1.3 (2026-10-18)
- In a convoy, WestPlay's clock, day and night, weather times and arrival times follow the convoy's time,
  as the cab's own clock does, instead of your own game's.
- Preparing a map shows how far it is all the way through, not only at a few points.

## 1.1.2 (2026-10-18)
- ProMods 2.84's map can now be prepared: its older parts, which 1.1.1 still couldn't read, are read too
  (checked against ProMods' own map, all 1,247 of its parts).
- If the screen mod can't be installed, the message says which step failed and where.

## 1.1.1 (2026-10-18)
- Maps from map mods made with an older version of the game's editor, ProMods 2.84 among them, can be
  prepared. Water painted by those mods may be missing for now.
- WestPlay finds the game's folder when it's in OneDrive but Windows' Documents points elsewhere (or the
  other way round), so the screen mod installs and your profiles and maps are found.

## 1.1.0 (2026-10-18)
- WestPlay now appears only in the trucks it's made for: the DAF XF and XG 2021, the Scania S and R, the
  Iveco S-Way and the Renault T 2024. Other trucks (the Mercedes-Benz Actros MP4, say) keep their own screen,
  and holding the key no longer takes their mouse. More trucks come with the tablet.
- WestPlay prepares a map for every profile, one after another, starting with the one you play. A profile
  whose map can't be prepared no longer leaves your other profiles without one, and the Map page shows each
  profile's map: ready, being prepared, or why it couldn't be.
- The Map page has a Prepare Maps Again button: every profile's map is made anew, one by one, failed ones
  included. The cab keeps showing your map until the new one is ready.
- Maps by HVT are recognised whatever they're called, so WestPlay says they aren't allowed instead of failing
  to prepare the map.

## 1.0.4 (2026-10-18)
- Setup no longer counts the Feldbinder and Krone trailer packs as map DLCs.
- The tutorial at the end of setup reads better: rewritten in Turkish, and clearer in English and every
  other language.

## 1.0.3 (2026-10-18)
- Spotify no longer goes silent after a pause, or by itself between songs, when it plays through the truck's
  speakers.
- Uninstalling removes everything you asked it to, your maps and sign-ins included, and no window flashes up.
- Signing out of a service finishes properly, without the button spinning on.
- A truck from a mod (Jasper's DAF 2021, say) counts as installed even when no profile uses the mod yet.

## 1.0.2 (2026-10-17)
- You can join a friend's convoy, or have them join yours, when only one of you has WestPlay: its mod is
  marked as optional in convoys.

## 1.0.1 (2026-10-17)
- A new version of WestPlay is offered as soon as you open it, whichever page it opens on.
- If WestPlay can't check for updates (no internet, say), it tries again in half an hour instead of six hours.

## 1.0.0 (2026-10-17)
- First release.
