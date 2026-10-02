# Manta - A Doom-emacs-like notetaking app

## Roadmap, Todos & Planned Features
### Bugs/Issues
- [X] **Undo/Redo can crash the app**
- [ ] Welcome View is a placeholder
- [ ] Buffer::end_transaction is a no-op. Remove it
- [ ] Outdated treesitter grammar. Using nvim-orgmodes grammar should be better. Possibly via "https://crates.io/crates/treesitter-types"
- [ ] No soft line breaks

### Roadmap
- [ ] New plugin type: TUI based plugins:
  - [ ] Make buffer readonly 
  - [ ] Capture keybinds (like emacs/vim start screens "press [f] to open filefinder")

- [ ] Agenda plugin. Similar to emacses agenda. Calendar view, week/day views.
  - [ ] Sources multiple files/directories (specified in a config)
  - [ ] Merges them into a single view providing a timeline of the day alongside the tasks of the day
  
- [ ]  Caldav plugin
  - [ ] Implement the caldav protocol to source/sync calendars, todos, maybe contacts
  - [ ] Synced files should use the same interface like "Agenda" 
  - [ ] Extend the API to implement intervall based functions "source calendar every 5 mins"

- [ ] Captures
  - [ ] Like emacs. Quickly store nodes (append to file) via a quick pane (without opening a new file)

- [ ] Mail plugin
  - [ ] maybe via mu. Maybe via a rust crate
  - [ ] Maybe mails files that can be referenced/linked

- [ ] Rework input scheme
  - [ ] Move vim support into a plugin to allow "normal" text editing
  - [ ] Add editing popups, cursor highlighting, "/"-commands? 
  - [ ] Add keybinds

- [ ] Vim bar plugin
  - [ ] Current cursor position
  - [ ] Current mode (needs visual mode)
  - [ ] Make it more modular. Allow the user to place elements (center/left/right) via config.
  - [ ] Allow injection of views as modules or just text insertions?
  - [ ] Remove command bar and make it a plugin. Maybe rename it then to manta-bar?

- [ ] FileFinder
  - [ ] Add Selected Index
  - [ ] Add scrolling
  - [ ] Add File Icons
  - [ ] Add File Info (Maybe rights, size)
  - [ ] Add configs
  - [ ] Add recent files

- [ ] Typst
  - [ ] requires System install use the crate instead
  - [ ] Add configs

- [ ] Toggable GUIS - Since a gui frame is availabe, let's use it for "nicer" looking UIs like the current start-screen
  - [ ] Nice UI for calendar/agenda
  - [ ] Nice UI for mail
  - [ ] Nice UI for Commands (like VSCode)
  - [ ] Nice UI for FileFinder (like telescope in nvim)

- [ ] Welcome View
  - [ ] Finish UI
  - [ ] Add quick actions (Open File Finder, Recent Files, Last opened file)

- [ ] Manta configs
  - [ ] Extend current theme capabilities
    - [ ] More color fields (Base16???)
    - [ ] UI-Font/Text Font
  - [ ] Startup command (enables different start views)
  - [ ] Enable/Disable plugins
  - [ ] Default Workspace directory (without arguments manta starts from there)

- [ ] Editor Commands where mutliple listeners can register (e.g. reload config, file-opened)
- [ ] Editor Commands with parameters (cursor_moved current_position)
- [ ] Continuous implementation of vim binds
- [ ] Continuous implementation of .org/(later markdown) elements
- [ ] Editor splits (mutliple files open)/Tabs? (Niri like infinte horizontal band?)
- [ ] Enable more bars (side,top,bottom) maybe mutliple bottom bars? 
- [ ] Add Markdown support
- [ ] Text search

### Plugin ideas 
  - [ ] Text replacements. (e.g. `- [ ]` to a clickable checkbox, image path to image) I want to keep the base editor dumb, so replacements like that will probably be done by plugins
  - [ ] Settings GUI 
  - [ ] Executbale code blocks
  - [ ] Spotify/Music Plugin
  - [ ] Pomodoro Plugin
  - [ ] latex plugin (instead of typst. for latex lovers)
  - [ ] Export plugin (Export a note to pdf,(latex?/typst?), epub)

### Still in planning
- Google Calendar support
- Links (via DB?)
- Make config accessible via manta-api.
- Single config file vs per-plugin config?
- WASM model for extensions.
- Typed errors over current (unhandled) String errors.
