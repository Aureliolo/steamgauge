// Stands in for the core behind the window: every command the window can invoke answers from
// the fixtures below, shaped as the Rust side serialises them, and every call is recorded so the
// check can ask what the window asked for. Loaded before the window's own script.
(function () {
  const DAY = 86_400;
  const now = 1_759_500_000;

  const subject = (id, label, reviews, praised, criticised, mixed, terms) => ({
    id,
    label,
    reviews,
    rate: reviews / 20_000,
    praised,
    criticised,
    mixed,
    claims: reviews + 40,
    top_rate: 0.1,
    bias: 1.3,
    positive: 0.7,
    corrected: null,
    found: null,
    praised_terms: terms ? [{ text: 'smooth', reviews: 41 }] : [],
    criticised_terms: terms ? [{ text: 'stutters', reviews: 37 }] : [],
  });

  const counts = (id, label, mentions, praised, criticised, mixed) => ({
    id,
    label,
    mention_reviews: mentions,
    primary_reviews: mentions / 2,
    claims: mentions + 10,
    praised,
    criticised,
    mixed,
    top_mention_reviews: 3,
    positive_mentions: praised,
  });

  const shift = (before, recent, z) => ({
    before,
    recent,
    before_reviews: 5_000,
    recent_reviews: 900,
    z,
  });

  const games = [
    {
      app_id: 1,
      name: 'Alpha',
      reviews: 20_000,
      valve_total: 21_000,
      coverage: 0.95,
      verdict: 'Very Positive',
      recommended: 0.86,
      downloaded: now - 40 * DAY,
      updated: now - 3 * DAY,
      new_on_steam: 1_520,
      read: { language: 'english', reviews: 15_000, claims: 60_000, current: true, reader: 'Game Review Reader', recommended: 0.85 },
      recent: {
        from: '2025-08',
        to: '2025-10',
        since: '2024-08',
        recommended: shift(0.88, 0.8, -5.2),
        moves: [{ subject: 'performance', label: 'Performance', side: 'complaint', shift: shift(0.12, 0.31, 9.1) }],
      },
      groups: [],
    },
    {
      app_id: 2,
      name: 'Beta',
      reviews: 5_000,
      valve_total: 5_100,
      coverage: 0.98,
      verdict: 'Mixed',
      recommended: 0.55,
      downloaded: now - 90 * DAY,
      updated: null,
      new_on_steam: 0,
      read: { language: null, reviews: 5_000, claims: 18_000, current: false, reader: 'Game Review Reader', recommended: 0.55 },
      recent: { from: '2025-08', to: '2025-10', since: '2024-08', recommended: null, moves: [] },
      groups: ['Rivals'],
    },
    {
      app_id: 3,
      name: 'Gamma',
      reviews: 900,
      valve_total: 950,
      coverage: 0.94,
      verdict: 'Positive',
      recommended: 0.75,
      downloaded: now - 2 * DAY,
      updated: null,
      new_on_steam: null,
      read: null,
      recent: null,
      groups: [],
    },
  ];

  let groups = [{ name: 'Rivals', app_ids: [2] }];
  let settings = {
    gpu_share: 0.5,
    reader: null,
    language: 'english',
    check_steam: true,
    read_after_download: true,
  };
  let searchEveryGame = false;
  let board = [];
  let nextJob = 100;
  const listeners = new Map();
  const calls = [];

  const reading = (appId) => {
    const game = games.find((one) => one.app_id === appId);
    if (!game.read) throw new Error('this game has not been read yet; run the reading pass first');
    return {
      app_id: appId,
      name: game.name,
      reviews: game.read.reviews,
      corpus_reviews: game.reviews,
      language: game.read.language,
      claims: game.read.claims,
      unclassified_claims: 9_000,
      silent_reviews: 400,
      top_of_the_pile: 50,
      positive_baseline: game.read.recommended,
      model: 'intfloat/multilingual-e5-large-instruct',
      threshold: 0.37,
      in_short: 'People mostly talk about performance and the story.',
      swept_since: null,
      subjects: [
        subject('performance', 'Performance', 4_000, 900, 2_600, 300, true),
        subject('story', 'Story', 3_000, 2_400, 300, 200, false),
      ],
      measured: null,
      learned: false,
      frozen: { games: 10, claims: 5_080, coverage: 0.98, accuracy: 0.78 },
      months: [
        { label: '2025-08', name: 'Aug 2025', reviews: 300, positive: 0.8 },
        { label: '2025-09', name: 'Sep 2025', reviews: 500, positive: 0.75 },
      ],
      languages: [{ name: 'english', reviews: 15_000, share: 0.75 }],
    };
  };

  const send = (event, payload) => {
    for (const handler of listeners.get(event) ?? []) handler({ event, payload });
  };

  const queued = (task) => {
    const job = {
      id: (nextJob += 1),
      task,
      name: games.find((one) => one.app_id === task.app_id)?.name ?? 'Your library',
      state: 'queued',
      step: 'Waiting',
      unit: 'reviews',
      done: 0,
      total: null,
      rate: null,
      left: null,
      note: null,
      queued: now,
      started: null,
      ended: null,
    };
    board = [...board, job];
    send('work', board);
    return job.id;
  };

  const answers = {
    work: () => board,
    queue: ({ tasks }) => tasks.map(queued),
    queue_reads: ({ appIds }) => appIds.map((app_id) => queued({ kind: 'read', app_id, language: 'english' })),
    queue_updates: ({ appIds }) =>
      (appIds ?? games.map((game) => game.app_id)).map((app_id) => queued({ kind: 'update', app_id })),
    stop_job: ({ id }) => {
      board = board.map((job) => (job.id === id ? { ...job, state: 'stopped', note: 'Stopped.' } : job));
      send('work', board);
    },
    clear_finished: () => {
      board = board.filter((job) => job.state === 'queued' || job.state === 'running');
      send('work', board);
    },
    open_report: () => null,
    overview: () => ({
      games: games.length,
      read: 2,
      reviews: 25_900,
      claims: 78_000,
      disk_bytes: 22_400_000_000,
      library: 'C:\\Users\\someone\\AppData\\Roaming\\com.aureliolo.steamgauge\\data',
      not_read: [{ app_id: 3, name: 'Gamma' }],
      older_reader: [{ app_id: 2, name: 'Beta' }],
      new_on_steam: [{ app_id: 1, name: 'Alpha', new: 1_520 }],
      checked: now - DAY,
      moves: [
        {
          app_id: 1,
          name: 'Alpha',
          from: '2025-08',
          to: '2025-10',
          since: '2024-08',
          subject: 'performance',
          label: 'Performance',
          side: 'complaint',
          shift: shift(0.12, 0.31, 9.1),
        },
      ],
      recommended: [
        { app_id: 1, name: 'Alpha', from: '2025-08', to: '2025-10', since: '2024-08', shift: shift(0.88, 0.8, -5.2) },
      ],
      machine: {
        card: 'NVIDIA GeForce RTX 4090',
        card_bytes: 25_769_803_776,
        reaches_card: true,
        on_processor: false,
        reader: 'standard',
        gpu_share: settings.gpu_share,
        models: [
          { name: 'Game Review Reader (small)', role: 'reads every review', here: false, bytes_left: 244_000_000, release: 'v1', newer: null, used: false },
          { name: 'Game Review Reader (standard)', role: 'reads every review', here: true, bytes_left: 0, release: 'v1', newer: 'v2', used: true },
          { name: 'SteamGauge search encoder', role: 'finds what was said in other words', here: false, bytes_left: 1_321_804_224, release: 'v1', newer: null, used: true },
        ],
        releases_checked: now - DAY,
      },
      jobs: board,
    }),
    games: () => games,
    groups: () => ({ groups }),
    save_groups: ({ groups: saved }) => {
      groups = saved.groups;
      for (const game of games) {
        game.groups = groups.filter((one) => one.app_ids.includes(game.app_id)).map((one) => one.name);
      }
      return { groups };
    },
    compare: ({ appIds }) =>
      appIds.map((appId) => {
        const game = games.find((one) => one.app_id === appId);
        return {
          app_id: appId,
          name: game.name,
          reviews: game.read?.reviews ?? 0,
          language: game.read?.language ?? null,
          recommended: game.read?.recommended ?? null,
          subjects:
            appId === 1
              ? [
                  counts('performance', 'Performance', 4_000, 900, 2_600, 300),
                  counts('story', 'Story', 3_000, 2_400, 300, 200),
                  counts('vr', 'VR', 0, 0, 0, 0),
                ]
              : [
                  counts('performance', 'Performance', 600, 150, 400, 30),
                  counts('story', 'Story', 1_500, 1_200, 200, 50),
                  counts('vr', 'VR', 0, 0, 0, 0),
                ],
        };
      }),
    export_report: ({ appIds }) => queued({ kind: 'export', app_ids: appIds, to: 'C:\\report.html' }),
    settings: () => ({
      ...settings,
      search_every_game: searchEveryGame,
      shares: [0.25, 0.5, 0.75, 1],
      library: 'C:\\Users\\someone\\AppData\\Roaming\\com.aureliolo.steamgauge\\data',
    }),
    save_settings: ({ settings: saved, searchEveryGame: every }) => {
      settings = saved;
      searchEveryGame = every;
      return answers.settings();
    },
    read_offer: ({ appId }) => ({
      reader: 'standard',
      download_bytes: appId === 3 ? 1_127_000_000 : 0,
      published: true,
      here: appId !== 3,
      choices: [],
    }),
    library: () => ({
      path: 'C:\\data',
      games: games.map((game) => ({
        app_id: game.app_id,
        name: game.name,
        reviews: game.reviews,
        valve_total: game.valve_total,
        coverage: game.coverage,
        verdict: game.verdict,
        snapshot: game.downloaded,
        stage: game.read ? 'read' : 'crawled',
      })),
    }),
    reading: ({ appId }) => reading(appId),
    induced: () => [],
    look_up: ({ appId }) => ({
      app_id: appId,
      name: 'Delta',
      reviews: 12_000,
      positive: 10_000,
      negative: 2_000,
      verdict: 'Very Positive',
      held: false,
    }),
    claims_behind: () => ({ subject: 'performance', total: 1, from: 0, claims: [] }),
    meaning_offer: () => ({
      status: 'none',
      on_card: true,
      seconds: 120,
      disk_bytes: 1_000_000,
      download_bytes: 0,
      every_game: false,
      job: null,
      recommended: 'prepare',
    }),
  };

  window.__TAURI__ = {
    core: {
      invoke: async (command, args = {}) => {
        calls.push({ command, args });
        const answer = answers[command];
        if (!answer) throw new Error(`the stub has no answer for ${command}`);
        return answer(args);
      },
    },
    event: {
      listen: async (event, handler) => {
        listeners.set(event, [...(listeners.get(event) ?? []), handler]);
        return () => {};
      },
    },
  };

  // What the check reaches for: the calls made, and a way to move the board as the core would.
  window.__stub = {
    calls,
    send,
    board: (jobs) => {
      board = jobs;
      send('work', board);
    },
  };
})();
