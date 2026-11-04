// Stands in for the core behind the window: every command the window can invoke answers from
// the fixtures below, shaped as the Rust side serialises them, and every call is recorded so the
// check can ask what the window asked for. Loaded before the window's own script.
const STUB_DELTA_SEARCH = /delta/iu;

(() => {
  const Day = 86_400;
  const now = 1_759_500_000;

  const subjectRow = ({ id, label, reviews, praised, criticised, mixed, terms }) => ({
    bias: 1.3,
    claims: reviews + 40,
    corrected: null,
    criticised,
    criticised_terms: terms ? [{ reviews: 37, text: 'stutters' }] : [],
    found: null,
    id,
    label,
    mixed,
    positive: 0.7,
    praised,
    praised_terms: terms ? [{ reviews: 41, text: 'smooth' }] : [],
    rate: reviews / 20_000,
    reviews,
    top_rate: 0.1,
  });

  const theme = ({ subjectId, index, label, quote, reviews, alsoIn = [] }) => ({
    also_in: alsoIn,
    label,
    quote,
    reviews,
    share: reviews / 20_000,
    subject: subjectId,
    subject_label: subjectId === 'story' ? 'Story' : 'Performance',
    theme: index,
  });
  // Seven praised, so the first six leave one behind the way to see them all.
  const themes = {
    complaint: [
      theme({
        subjectId: 'performance',
        index: 0,
        label: 'Stutters in towns.',
        quote: 'The frame rate stutters in every town.',
        reviews: 700,
      }),
      theme({ subjectId: 'story', index: 0, label: 'Act two drags.', quote: null, reviews: 250 }),
    ],
    praise: [
      theme({
        subjectId: 'story',
        index: 0,
        label: 'The writing is sharp.',
        quote: 'Every line of the writing is sharp and funny.',
        reviews: 1200,
      }),
      theme({
        subjectId: 'performance',
        index: 0,
        label: 'Runs smooth.',
        quote: null,
        reviews: 900,
        alsoIn: ['Story'],
      }),
      theme({
        subjectId: 'story',
        index: 1,
        label: 'The ending lands.',
        quote: null,
        reviews: 600,
      }),
      theme({
        subjectId: 'story',
        index: 2,
        label: 'Great characters.',
        quote: null,
        reviews: 400,
      }),
      theme({
        subjectId: 'performance',
        index: 1,
        label: 'Loads fast.',
        quote: null,
        reviews: 300,
      }),
      theme({ subjectId: 'story', index: 3, label: 'Funny dialogue.', quote: null, reviews: 200 }),
      theme({
        subjectId: 'performance',
        index: 2,
        label: 'Works on old cards.',
        quote: null,
        reviews: 90,
      }),
    ],
  };

  const counts = ({ id, label, mentions, praised, criticised, mixed }) => ({
    claims: mentions + 10,
    criticised,
    id,
    label,
    mention_reviews: mentions,
    mixed,
    positive_mentions: praised,
    praised,
    primary_reviews: mentions / 2,
    top_mention_reviews: 3,
  });

  // A gap between two kinds of reviewer, marked as the core marks one.
  const gap = (share, against, z) => ({
    against,
    clear: Math.abs(z) >= 4 && Math.abs(share - against) >= 0.02,
    share,
    z,
  });

  // The kinds of reviewer a read game was counted for, as the core lists them, and how a
  // sentence names each.
  const splitKind = (id, label, reviews, recommended) => ({
    enough: reviews >= 100,
    id,
    label,
    recommended,
    reviews,
  });
  const kinds = () => [
    {
      id: 'played',
      kinds: [
        splitKind('under-2-hours', 'Under 2 hours', 1650, gap(0.62, 0.87, -14.2)),
        splitKind('2-to-10-hours', '2 to 10 hours', 3450, gap(0.83, 0.86, -2.1)),
        splitKind('10-to-30-hours', '10 to 30 hours', 3750, gap(0.88, 0.84, 3.0)),
        splitKind('30-to-100-hours', '30 to 100 hours', 3450, gap(0.9, 0.84, 5.3)),
        splitKind('100-hours-or-more', '100 hours or more', 2700, gap(0.91, 0.84, 6.1)),
      ],
      label: 'Time played when they wrote it',
    },
    {
      id: 'deck',
      kinds: [
        splitKind('steam-deck', 'Mostly on a Steam Deck', 12, null),
        splitKind('elsewhere', 'Mostly elsewhere', 14_988, null),
      ],
      label: 'Where they played',
    },
    {
      id: 'early-access',
      kinds: [
        splitKind('early-access', 'During early access', 4500, gap(0.81, 0.87, -6.4)),
        splitKind('after-release', 'After release', 10_500, gap(0.87, 0.81, 6.4)),
      ],
      label: 'When they wrote it',
    },
    {
      id: 'copy',
      kinds: [
        splitKind('got-it-free', 'Got it free', 600, gap(0.86, 0.85, 0.7)),
        splitKind('paid-for-it', 'Paid for it', 14_400, gap(0.85, 0.86, -0.7)),
      ],
      label: 'How they got the game',
    },
  ];
  const phrases = {
    '2-to-10-hours': 'Reviewers with 2 to 10 hours played',
    '10-to-30-hours': 'Reviewers with 10 to 30 hours played',
    '30-to-100-hours': 'Reviewers with 30 to 100 hours played',
    '100-hours-or-more': 'Reviewers with 100 hours or more played',
    'after-release': 'Reviewers writing after release',
    'early-access': 'Reviewers writing during early access',
    'got-it-free': 'Reviewers who got it free',
    'paid-for-it': 'Reviewers who paid for it',
    'under-2-hours': 'Reviewers with under 2 hours played',
  };
  const finding = ({ segment, label, subjectId, said, found, sentence }) => ({
    gap: found,
    label,
    said,
    segment,
    sentence,
    subject: subjectId,
  });
  const findings = () => [
    finding({
      segment: '100-hours-or-more',
      label: '100 hours or more',
      subjectId: 'performance',
      said: 'complains',
      found: gap(0.31, 0.12, 17.2),
      sentence:
        'Reviewers with 100 hours or more played complain about performance in 31.0% of their reviews, against 12.0% of everyone else.',
    }),
    finding({
      segment: 'under-2-hours',
      label: 'Under 2 hours',
      subjectId: null,
      said: 'recommends',
      found: gap(0.62, 0.87, -14.2),
      sentence:
        'Reviewers with under 2 hours played recommend the game in 62.0% of their reviews, against 87.0% of everyone else.',
    }),
    finding({
      segment: 'under-2-hours',
      label: 'Under 2 hours',
      subjectId: 'story',
      said: 'praises',
      found: gap(0.21, 0.09, 8.1),
      sentence:
        'Reviewers with under 2 hours played praise story and writing in 21.0% of their reviews, against 9.0% of everyone else.',
    }),
    finding({
      segment: 'early-access',
      label: 'During early access',
      subjectId: 'bugs',
      said: 'complains',
      found: gap(0.18, 0.07, 7.7),
      sentence:
        'Reviewers writing during early access complain about bugs and crashes in 18.0% of their reviews, against 7.0% of reviewers writing after release.',
    }),
    finding({
      segment: 'early-access',
      label: 'During early access',
      subjectId: null,
      said: 'recommends',
      found: gap(0.81, 0.87, -6.4),
      sentence:
        'Reviewers writing during early access recommend the game in 81.0% of their reviews, against 87.0% of reviewers writing after release.',
    }),
    finding({
      segment: '30-to-100-hours',
      label: '30 to 100 hours',
      subjectId: 'content',
      said: 'complains',
      found: gap(0.09, 0.05, 5.2),
      sentence:
        'Reviewers with 30 to 100 hours played complain about amount of content in 9.0% of their reviews, against 5.0% of everyone else.',
    }),
    finding({
      segment: '2-to-10-hours',
      label: '2 to 10 hours',
      subjectId: 'tutorial',
      said: 'praises',
      found: gap(0.06, 0.03, 4.4),
      sentence:
        'Reviewers with 2 to 10 hours played praise tutorial and learning in 6.0% of their reviews, against 3.0% of everyone else.',
    }),
  ];
  // One subject's figures among one side's reviews, with a band where its share would likely fall.
  const figures = ({ raised, reviews, praised, criticised, mixed }) => ({
    claims: raised * 2,
    complaining: (criticised + mixed) / reviews,
    criticised,
    high: Math.min(1, raised / reviews + 0.025),
    low: Math.max(0, raised / reviews - 0.025),
    mixed,
    praised,
    praising: (praised + mixed) / reviews,
    raised,
    rate: raised / reviews,
  });
  const head = ({ id, label, who, reviews, recommending }) => ({
    claims: reviews * 4,
    high: recommending + 0.02,
    id,
    label,
    low: recommending - 0.02,
    recommending,
    reviews,
    who,
  });

  const shift = (before, recent, z) => ({
    before,
    before_reviews: 5000,
    recent,
    recent_reviews: 900,
    z,
  });

  const games = [
    {
      app_id: 1,
      coverage: 0.95,
      downloaded: now - 40 * Day,
      groups: [],
      name: 'Alpha',
      new_on_steam: 1520,
      read: {
        claims: 60_000,
        current: true,
        language: 'english',
        reader: 'Game Review Reader',
        recommended: 0.85,
        reviews: 15_000,
      },
      recent: {
        from: '2025-08',
        moves: [
          {
            label: 'Performance',
            shift: shift(0.12, 0.31, 9.1),
            side: 'complaint',
            subject: 'performance',
          },
        ],
        recommended: shift(0.88, 0.8, -5.2),
        since: '2024-08',
        to: '2025-10',
      },
      recommended: 0.86,
      reviews: 20_000,
      updated: now - 3 * Day,
      valve_total: 21_000,
      verdict: 'Very Positive',
    },
    {
      app_id: 2,
      coverage: 0.98,
      downloaded: now - 90 * Day,
      groups: ['Rivals'],
      name: 'Beta',
      new_on_steam: 0,
      read: {
        claims: 18_000,
        current: false,
        language: null,
        reader: 'Game Review Reader',
        recommended: 0.55,
        reviews: 5000,
      },
      recent: { from: '2025-08', moves: [], recommended: null, since: '2024-08', to: '2025-10' },
      recommended: 0.55,
      reviews: 5000,
      updated: null,
      valve_total: 5100,
      verdict: 'Mixed',
    },
    {
      app_id: 3,
      coverage: 0.94,
      downloaded: now - 2 * Day,
      groups: [],
      name: 'Gamma',
      new_on_steam: null,
      read: null,
      recent: null,
      recommended: 0.75,
      reviews: 900,
      updated: null,
      valve_total: 950,
      verdict: 'Positive',
    },
  ];

  // What changed in a game since a look, as `since::Since` serialises it.
  const since = (fields) => ({
    from: null,
    looked: now - 2 * Day,
    moves: [],
    new: 0,
    read_new: 0,
    recommended: null,
    standing: 'nothing',
    to: null,
    ...fields,
  });
  const alphaSince = () =>
    since({
      from: '2024-09',
      moves: [
        {
          label: 'Performance',
          shift: shift(0.12, 0.24, 9.4),
          side: 'complaint',
          subject: 'performance',
        },
        {
          label: 'Story and writing',
          shift: shift(0.2, 0.14, -4.5),
          side: 'praise',
          subject: 'story',
        },
      ],
      new: 1520,
      read_new: 1140,
      recommended: shift(0.86, 0.79, -5.6),
      standing: 'compared',
      to: '2025-08',
    });
  // The cockpit's first card in each of its states: things moved, nothing did, and a first look.
  const lately = {
    calm: () => ({ games: [], looked: now - 2 * Day }),
    first: () => ({ games: [], looked: null }),
    moved: () => ({
      games: [
        { app_id: 1, name: 'Alpha', ...alphaSince() },
        { app_id: 2, name: 'Beta', ...since({ new: 260, standing: 'unread' }) },
        { app_id: 3, name: 'Gamma', ...since({ new: 35, standing: 'unread' }) },
      ],
      looked: now - 2 * Day,
    }),
  };
  let latelyShown = 'moved';
  // Each game's own line: Alpha moved, Beta has nothing new, and Gamma's page was never seen.
  const sinceLastLook = { 1: alphaSince, 2: () => since({}), 3: () => null };

  // Opened with ?first, the core of somebody who has just installed the app: nothing downloaded.
  const fresh = new URLSearchParams(location.search).has('first');
  if (fresh) {
    games.length = 0;
  }

  let groups = [{ app_ids: [2], name: 'Rivals' }];
  let settings = {
    answer_over_http: false,
    check_every_hours: 6,
    check_newer_version: true,
    check_steam: true,
    gpu_share: 0.5,
    http_port: 47_800,
    keep_up_to_date: true,
    language: 'english',
    notify_moves: false,
    read_after_download: true,
    reader: null,
    update_when: 'hundredth',
  };
  // The token the HTTP server answers with, drawn again by New token.
  let httpToken = '0f'.repeat(32);
  // The answer the core kept from the last question.
  let newer = {
    running: '0.1.0',
    url: 'https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0',
    version: '0.2.0',
  };
  // Where Update now stands: nothing under way, on a copy the window can update.
  let update = { state: 'idle', why: null };
  // When GitHub was last asked and last answered, and the release it names when asked next.
  // A null stands for GitHub out of reach.
  let askedAt = 1_792_000_000;
  let answeredAt = askedAt;
  let github = '0.2.0';
  const release = (version) => ({
    running: '0.1.0',
    url: `https://github.com/Aureliolo/steamgauge/releases/tag/v${version}`,
    version,
  });
  // The version this copy was updated from, until the window says what changed was seen.
  let updated = null;
  // What GitHub's releases API lists between two versions, newest first.
  const releases = [
    {
      changes: [
        'About lists what each newer version changes',
        'The app asks for a newer version each time it opens',
      ],
      version: '0.2.0',
    },
    { changes: [], version: '0.1.1' },
  ];
  let whatsNewAsked = 0;
  const about = () => ({
    answered: answeredAt,
    asked: askedAt,
    built_on: ['498 Rust crates', 'ONNX Runtime 1.31.0', 'DirectML 1.15.4'],
    by_itself: settings.check_newer_version,
    checking: 'https://github.com/Aureliolo/steamgauge/security/policy',
    details: [
      { name: 'Version', value: '0.1.0' },
      { name: 'System', value: 'Windows, x86-64' },
      { name: 'Installed with', value: 'the setup program' },
      { name: 'WebView', value: '141.0.3537.71' },
      {
        name: 'Build',
        value: "Checked on install. SteamGauge's release workflow built it from v0.1.0.",
      },
    ],
    issues: 'https://github.com/Aureliolo/steamgauge/issues/new/choose',
    latest: newer?.version ?? '0.1.0',
    licence: 'Apache-2.0',
    made_by: 'Aurelio Amoroso',
    newer,
    notes: 'https://github.com/Aureliolo/steamgauge/releases/tag/v0.1.0',
    notices: true,
    repository: 'https://github.com/Aureliolo/steamgauge',
    running: '0.1.0',
    site: 'https://aureliolo.github.io/steamgauge/',
    updated,
  });
  let searchEveryGame = true;
  // The MCP clients on this computer: one without the app, one starting another copy, and one
  // whose settings file does not parse.
  const program = 'C:\\Users\\someone\\AppData\\Local\\SteamGauge\\steamgauge.exe';
  let clients = [
    {
      file: 'C:\\Users\\someone\\.cursor\\mcp.json',
      id: 'cursor',
      name: 'Cursor',
      state: 'installed',
      // biome-ignore lint/suspicious/noThenProperty: `then` is the field name of the window's client records, which the stub mirrors.
      then: null,
    },
    {
      command: 'D:\\Old\\steamgauge.exe',
      file: 'C:\\Users\\someone\\AppData\\Roaming\\Claude\\claude_desktop_config.json',
      id: 'claude-desktop',
      name: 'Claude Desktop',
      state: 'elsewhere',
      // biome-ignore lint/suspicious/noThenProperty: `then` is the field name of the window's client records, which the stub mirrors.
      then: 'Quit Claude Desktop and open it again.',
    },
    {
      file: 'C:\\Users\\someone\\AppData\\Roaming\\Zed\\settings.json',
      id: 'zed',
      name: 'Zed',
      reason: 'Unexpected token on line 3 column 5',
      state: 'unreadable',
      // biome-ignore lint/suspicious/noThenProperty: `then` is the field name of the window's client records, which the stub mirrors.
      then: null,
    },
  ];
  const steerFrom = (id, state) => {
    const client = clients.find((one) => one.id === id);
    if (!client) {
      throw new Error(`SteamGauge knows no MCP client called ${id}`);
    }
    if (client.state === 'unreadable') {
      throw new Error(
        `${client.file} does not parse (${client.reason}), so SteamGauge was not added to it; fix the file, or add the entry by hand`,
      );
    }
    const { name, file, then } = client;
    clients = clients.map((one) => (one.id === id ? { file, id, name, state, then } : one));
  };
  // What each game takes on disk, by part, and the models in the cache.
  let rooms = games.map((game, at) => ({
    app_id: game.app_id,
    name: game.name,
    parts: [
      {
        earlier: 1_800_000_000,
        other: 0,
        partial: 120_000_000,
        reads: 1_100_000_000,
        reviews: 2_400_000_000,
        search: 3_900_000_000,
      },
      { earlier: 0, other: 0, partial: 0, reads: 300_000_000, reviews: 640_000_000, search: 0 },
      { earlier: 0, other: 0, partial: 40_000_000, reads: 0, reviews: 110_000_000, search: 0 },
    ][at % 3],
  }));
  const models = [
    {
      bytes: 0,
      key: 'small',
      name: 'Small reader',
      role: 'faster, 78% agreement at 80% coverage',
      used: false,
    },
    {
      bytes: 1_127_000_000,
      key: 'standard',
      name: 'Standard reader',
      role: '84% agreement at 80% coverage',
      used: true,
    },
    {
      bytes: 1_310_000_000,
      key: 'search-encoder',
      name: 'SteamGauge search encoder',
      role: 'finds what was said in other words',
      used: true,
    },
    {
      bytes: 0,
      key: 'search-reranker',
      name: 'SteamGauge search reranker',
      role: 'orders search results by meaning',
      used: true,
    },
  ];
  const storage = () => {
    const total = (held) => Object.values(held).reduce((sum, bytes) => sum + bytes, 0);
    const parts = { earlier: 0, other: 349_000, partial: 0, reads: 0, reviews: 0, search: 0 };
    for (const room of rooms) {
      for (const key of Object.keys(room.parts)) {
        parts[key] += room.parts[key];
      }
    }
    return {
      drives: [
        {
          free: 182_000_000_000,
          holds: ['library', 'models'],
          name: 'C:',
          total: 1_000_000_000_000,
        },
      ],
      games: rooms
        .map((room) => ({ ...room, parts: { ...room.parts }, total: total(room.parts) }))
        .sort((a, b) => b.total - a.total),
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      model_rooms: models.map((model) => ({ ...model })),
      models: 'C:\\Users\\someone\\AppData\\Local\\steamgauge\\models',
      parts,
    };
  };
  let board = [];
  let nextJob = 100;
  // What saving a game's data is refused with, or null where it is not.
  let refusal = null;
  // Whether every model is on this computer and current, as on a machine that has run a while.
  let modelsSettled = false;
  // Alpha's paragraph, or null where none is written for its themes.
  let written =
    'Players praise the sharp writing most, in 1,200 reviews. The clearest complaint is ' +
    'stuttering in towns, which 700 reviews raise.';
  let writing = false;
  const listeners = new Map();
  const calls = [];

  const reading = (appId) => {
    const game = games.find((one) => one.app_id === appId);
    if (!game.read) {
      throw new Error('this game has not been read yet; run the reading pass first');
    }
    return {
      app_id: appId,
      claims: game.read.claims,
      corpus_reviews: game.reviews,
      held_out: { accuracy: 0.78, claims: 5080, coverage: 0.98, games: 10 },
      language: game.read.language,
      languages: [{ name: 'english', reviews: 15_000, share: 0.75 }],
      learned: false,
      measured: null,
      model: 'intfloat/multilingual-e5-large-instruct',
      months: calendar,
      name: game.name,
      // Beta is not prepared for search by meaning, so its themes are still to find.
      paragraph: appId === 1 ? written : null,
      positive_baseline: game.read.recommended,
      reviews: game.read.reviews,
      silent_reviews: 400,
      subjects: [
        subjectRow({
          id: 'performance',
          label: 'Performance',
          reviews: 4000,
          praised: 900,
          criticised: 2600,
          mixed: 300,
          terms: true,
        }),
        subjectRow({
          id: 'story',
          label: 'Story',
          reviews: 3000,
          praised: 2400,
          criticised: 300,
          mixed: 200,
          terms: false,
        }),
      ],
      swept_since: null,
      themes: appId === 1 ? themes : null,
      threshold: 0.37,
      top_of_the_pile: 50,
      unclassified_claims: 9000,
      // No kind of Beta's reviewers differs from everyone else by more than chance.
      who: { findings: appId === 1 ? findings() : [], kinds: kinds() },
    };
  };

  // One kind of reviewer beside everyone else or beside another kind, as `who_wrote` answers.
  const whoWrote = ({ these, others }) => {
    const all = kinds().flatMap((split) => split.kinds.map((one) => ({ ...one, split: split.id })));
    const first = all.find((one) => one.id === these);
    if (!first?.enough) {
      throw new Error(`${first?.label ?? these}: too few reviews to count`);
    }
    const second = others ? all.find((one) => one.id === others) : null;
    const rest = all
      .filter((one) => one.split === first.split && one.id !== first.id)
      .reduce((sum, one) => sum + one.reviews, 0);
    const against = second ? second.reviews : rest;
    // Counts written for everyone else, scaled to whoever the first kind is set beside.
    const scaled = (count) => Math.round((count * against) / 12_300);
    return {
      months: [
        { label: '2025-07', name: 'Jul 2025', positive: 0.84, reviews: 45 },
        { label: '2025-08', name: 'Aug 2025', positive: 0.9, reviews: 60 },
        { label: '2025-09', name: 'Sep 2025', positive: null, reviews: 20 },
      ],
      others: second
        ? head({
            id: second.id,
            label: second.label,
            who: phrases[second.id],
            reviews: second.reviews,
            recommending: 0.62,
          })
        : head({
            id: 'everyone-else',
            label: 'Everyone else',
            who: 'Everyone else',
            reviews: rest,
            recommending: 0.84,
          }),
      recommended: gap(0.91, 0.84, 6.1),
      split: first.split,
      subjects: [
        {
          complaint: gap(0.31, 0.15, 17.2),
          id: 'performance',
          label: 'Performance',
          others: figures({
            raised: scaled(2900),
            reviews: against,
            praised: scaled(700),
            criticised: scaled(1700),
            mixed: scaled(160),
          }),
          others_corrected: null,
          praise: gap(0.107, 0.07, 3.1),
          raised: gap(0.37, 0.24, 13.0),
          these: figures({
            raised: 1000,
            reviews: first.reviews,
            praised: 150,
            criticised: 700,
            mixed: 140,
          }),
          these_corrected: null,
        },
        {
          complaint: gap(0.026, 0.023, 0.8),
          id: 'story',
          label: 'Story and writing',
          others: figures({
            raised: scaled(1900),
            reviews: against,
            praised: scaled(1500),
            criticised: scaled(200),
            mixed: scaled(90),
          }),
          others_corrected: null,
          praise: gap(0.12, 0.13, -0.9),
          raised: gap(0.15, 0.15, 0.2),
          these: figures({
            raised: 400,
            reviews: first.reviews,
            praised: 300,
            criticised: 50,
            mixed: 20,
          }),
          these_corrected: null,
        },
        {
          complaint: gap(0, 0, 0),
          id: 'vr',
          label: 'VR and headsets',
          others: figures({
            raised: 0,
            reviews: against,
            praised: 0,
            criticised: 0,
            mixed: 0,
          }),
          others_corrected: null,
          praise: gap(0, 0, 0),
          raised: gap(0, 0, 0),
          these: figures({
            raised: 0,
            reviews: first.reviews,
            praised: 0,
            criticised: 0,
            mixed: 0,
          }),
          these_corrected: null,
        },
      ],
      these: head({
        id: first.id,
        label: first.label,
        who: phrases[first.id],
        reviews: first.reviews,
        recommending: 0.91,
      }),
    };
  };

  // Fourteen months, September 2024 to October 2025, as a reading draws them.
  const calendar = Array.from({ length: 14 }, (_, at) => {
    const month = new Date(Date.UTC(2024, 8 + at, 15));
    return {
      label: `${month.getUTCFullYear()}-${String(month.getUTCMonth() + 1).padStart(2, '0')}`,
      // The core writes every month in three letters.
      name: month
        .toLocaleDateString('en-GB', { month: 'short', timeZone: 'UTC', year: 'numeric' })
        .replace('Sept', 'Sep'),
      positive: 0.72 + ((at * 7) % 10) / 100,
      reviews: 300 + ((at * 137) % 500),
    };
  });

  // Alpha's updates as the core sends them, each with its month and how far through it: one
  // before the calendar starts, one alone, three posted within days of each other, one with too
  // few reviews either side, and one the download holds only nine days after.
  const posting = (gid, title, [year, month, day]) => ({
    gid,
    link: `https://store.steampowered.com/news/externalpost/steam_community_announcements/${gid}`,
    month: `${year}-${String(month).padStart(2, '0')}`,
    posted: Date.UTC(year, month - 1, day, 12) / 1000,
    through: (day - 0.5) / new Date(Date.UTC(year, month, 0)).getUTCDate(),
    title,
  });
  const alphaUpdates = [
    posting('1840000000000000', 'Early Access Patch 0.9', [2024, 6, 1]),
    posting('1840000000000001', 'Patch 1.1', [2024, 11, 12]),
    posting('1840000000000002', 'The Winter Update 1.2', [2025, 1, 14]),
    posting('1840000000000003', 'Hotfix 1.2.1', [2025, 1, 16]),
    posting('1840000000000004', 'Hotfix 1.2.2', [2025, 1, 17]),
    posting('1840000000000005', 'Patch 1.3', [2025, 6, 3]),
    posting('1840000000000006', 'Patch 1.4: Performance and Stability', [2025, 9, 30]),
  ];

  const movement = (before, after) => (after > before ? 4.2 : -4.2);
  const compared = (before, after, change) => ({
    after,
    before,
    change,
    z: change ? movement(before, after) : 0.6,
  });
  const side = (subjectId, label, praise, complaint) => ({
    complaint,
    label,
    praise,
    subject: subjectId,
  });
  const steady = [
    side('performance', 'Performance', compared(0.22, 0.23, false), compared(0.14, 0.13, false)),
    side('story', 'Story', compared(0.3, 0.31, false), compared(0.05, 0.05, false)),
  ];
  // What changed across each of Alpha's updates, by its id.
  const across = {
    1840000000000001: {
      nearby: 0,
      recommended: compared(0.82, 0.83, false),
      reviews: [980, 1020],
      subjects: steady,
    },
    1840000000000002: {
      nearby: 2,
      recommended: compared(0.84, 0.71, true),
      reviews: [1840, 2960],
      subjects: [
        side('bugs', 'Bugs and crashes', compared(0.04, 0.03, false), compared(0.08, 0.19, true)),
        side('performance', 'Performance', compared(0.22, 0.31, true), compared(0.14, 0.13, false)),
        side('story', 'Story', compared(0.3, 0.31, false), compared(0.05, 0.05, false)),
      ],
    },
    1840000000000003: {
      nearby: 2,
      recommended: compared(0.83, 0.72, true),
      reviews: [1900, 2700],
      subjects: steady,
    },
    1840000000000004: {
      nearby: 2,
      recommended: compared(0.81, 0.73, true),
      reviews: [2050, 2400],
      subjects: steady,
    },
    1840000000000005: { nearby: 0, recommended: null, reviews: [64, 41], subjects: [] },
    1840000000000006: {
      days: 9,
      nearby: 0,
      recommended: compared(0.8, 0.79, false),
      reviews: [1200, 410],
      subjects: steady,
    },
  };
  // One kind of reviewer wrote a sixth of the reviews either side.
  const beforeAfter = (appId, gid, kind) => {
    const entry = alphaUpdates.find((one) => one.gid === gid);
    const found = across[gid];
    if (appId !== 1 || !entry || !found) {
      throw new Error(`no update ${gid} is kept for app ${appId}`);
    }
    const window = 28 * Day;
    const after = found.days ? found.days * Day : window;
    const reviews = kind ? found.reviews.map((count) => Math.round(count / 6)) : found.reviews;
    const enough = reviews[0] >= 100 && reviews[1] >= 100;
    return {
      after: { from: entry.posted, reviews: reviews[1], to: entry.posted + after },
      after_whole: !found.days,
      before: { from: entry.posted - window, reviews: reviews[0], to: entry.posted },
      changes: found.subjects.reduce(
        (sum, one) => sum + one.praise.change + one.complaint.change,
        0,
      ),
      enough,
      nearby: found.nearby,
      recommended: found.recommended,
      subjects: found.subjects,
      update: { gid: entry.gid, link: entry.link, posted: entry.posted, title: entry.title },
    };
  };

  const comparedGame = (appId) => {
    const game = games.find((one) => one.app_id === appId);
    return {
      app_id: appId,
      language: game.read?.language ?? null,
      name: game.name,
      recommended: game.read?.recommended ?? null,
      reviews: game.read?.reviews ?? 0,
      subjects:
        appId === 1
          ? [
              counts({
                id: 'performance',
                label: 'Performance',
                mentions: 4000,
                praised: 900,
                criticised: 2600,
                mixed: 300,
              }),
              counts({
                id: 'story',
                label: 'Story',
                mentions: 3000,
                praised: 2400,
                criticised: 300,
                mixed: 200,
              }),
              counts({
                id: 'audio',
                label: 'Audio',
                mentions: 1200,
                praised: 900,
                criticised: 200,
                mixed: 100,
              }),
              counts({ id: 'vr', label: 'VR', mentions: 0, praised: 0, criticised: 0, mixed: 0 }),
            ]
          : [
              counts({
                id: 'performance',
                label: 'Performance',
                mentions: 600,
                praised: 150,
                criticised: 400,
                mixed: 30,
              }),
              counts({
                id: 'story',
                label: 'Story',
                mentions: 1500,
                praised: 1200,
                criticised: 200,
                mixed: 50,
              }),
              // Too few to say how they split, unless the person asks.
              counts({
                id: 'audio',
                label: 'Audio',
                mentions: 20,
                praised: 0,
                criticised: 20,
                mixed: 0,
              }),
              counts({ id: 'vr', label: 'VR', mentions: 0, praised: 0, criticised: 0, mixed: 0 }),
            ],
    };
  };

  const send = (event, payload) => {
    for (const handler of listeners.get(event) ?? []) {
      handler({ event, payload });
    }
  };

  const queued = (task) => {
    nextJob += 1;
    const job = {
      background: false,
      done: 0,
      ended: null,
      id: nextJob,
      left: null,
      name: games.find((one) => one.app_id === task.app_id)?.name ?? 'Your library',
      note: null,
      queued: now,
      rate: null,
      started: null,
      state: 'queued',
      step: 'Waiting',
      task,
      total: null,
      unit: 'reviews',
    };
    board = [...board, job];
    send('work', board);
    return job.id;
  };

  // What a figure is on a fresh install with no games and what it is once the library holds some.
  const whenLibrary = (empty, held) => (games.length === 0 ? empty : held);

  const httpAnswer = () => {
    if (!settings.answer_over_http) {
      return { address: null, problem: null, token: null };
    }
    // Port 47801 stands in for one another program holds.
    if (settings.http_port === 47_801) {
      return {
        address: null,
        problem: 'Nothing answers on port 47801: another program has it; choose another port',
        token: null,
      };
    }
    return {
      address: `http://127.0.0.1:${settings.http_port}/mcp`,
      problem: null,
      token: httpToken,
    };
  };

  const answers = {
    about: () => about(),
    // A picture drawn here, so a game's art is shown without the window reaching anywhere.
    art: async ({ appId }) => {
      const canvas = document.createElement('canvas');
      canvas.width = 460;
      canvas.height = 215;
      const paint = canvas.getContext('2d');
      const hue = (appId * 67) % 360;
      const fill = paint.createLinearGradient(0, 0, 460, 215);
      fill.addColorStop(0, `hsl(${hue} 55% 35%)`);
      fill.addColorStop(1, `hsl(${(hue + 50) % 360} 60% 55%)`);
      paint.fillStyle = fill;
      paint.fillRect(0, 0, 460, 215);
      const picture = await new Promise((done) => canvas.toBlob(done, 'image/png'));
      return picture.arrayBuffer();
    },
    before_after: ({ appId, gid, kind }) => beforeAfter(appId, gid, kind),
    check_for_updates: () => {
      askedAt += 60;
      if (github !== null) {
        answeredAt = askedAt;
        newer = github === '0.1.0' ? null : release(github);
      }
      send('newer-version', newer);
      return about();
    },
    choose_writer: ({ write }) => {
      writing = write;
    },
    claims_behind: () => ({
      claims: [
        {
          claim: 'It runs smooth at 144 fps on a mid-range card.',
          confidence: 0.94,
          created: 1_756_000_000,
          language: 'english',
          polarity: 'praise',
          review:
            'Great combat. It runs smooth at 144 fps on a mid-range card. The story drags in act two.',
          url: 'https://steamcommunity.com/profiles/1/recommended/1/',
          voted_up: true,
          votes_up: 12,
        },
        {
          claim: 'The frame rate stutters in every town.',
          confidence: 0.88,
          created: 1_757_000_000,
          language: 'english',
          polarity: 'complaint',
          review: 'The frame rate stutters in every town.',
          url: 'https://steamcommunity.com/profiles/2/recommended/1/',
          voted_up: false,
          votes_up: 0,
        },
      ],
      from: 0,
      subject: 'performance',
      total: 2,
    }),
    clear_finished: () => {
      board = board.filter((job) => job.state === 'queued' || job.state === 'running');
      send('work', board);
    },
    compare: ({ appIds }) => appIds.map(comparedGame),
    connect_client: ({ client }) => {
      steerFrom(client, 'added');
      return answers.settings();
    },
    disconnect_client: ({ client }) => {
      steerFrom(client, 'installed');
      return answers.settings();
    },
    // The core asks where in the system's dialog. A refusal stands in for a name already taken.
    export_data: ({ appId }) => {
      if (refusal) {
        throw new Error(refusal);
      }
      return queued({ app_id: appId, kind: 'export_data', to: 'C:\\Alpha - SteamGauge data' });
    },
    export_report: ({ appIds }) =>
      queued({ app_ids: appIds, kind: 'export', to: 'C:\\report.html' }),
    find_games: ({ words }) =>
      STUB_DELTA_SEARCH.test(words)
        ? [
            { app_id: 4, name: 'Delta' },
            { app_id: 5, name: 'Delta: The Expansion' },
          ]
        : [],
    free_room: ({ appId, what }) => {
      const parts =
        { earlier: ['earlier'], partial: ['partial'], reads: ['reads'], search: ['search'] }[
          what
        ] ?? Object.keys(rooms[0].parts);
      for (const room of rooms) {
        if (appId !== null && room.app_id !== appId) {
          continue;
        }
        for (const part of parts) {
          room.parts[part] = 0;
        }
      }
      if (what === 'game') {
        rooms = rooms.filter((room) => room.app_id !== appId);
      }
      return storage();
    },
    // Alpha's developer posts updates. Beta was never asked about. Anything else posted none.
    game_updates: ({ appId }) => ({
      asked: appId === 2 ? null : now - Day,
      enough: 100,
      updates: appId === 1 ? alphaUpdates : [],
      window_days: 28,
    }),
    games: () => games,
    // Figures worked out as `steamgauge_core::group::across` does, from the comparison's counts.
    group_reading: ({ name }) => {
      const group = groups.find((one) => one.name === name);
      if (!group) {
        throw new Error(`no group is called ${name}`);
      }
      const read = group.app_ids.filter((appId) => games.find((one) => one.app_id === appId)?.read);
      const found = read.map(comparedGame);
      const ratio = (part, whole) => (whole > 0 ? part / whole : null);
      const mean = (values) =>
        values.length > 0 ? values.reduce((sum, value) => sum + value, 0) / values.length : null;
      const subjects = found[0]?.subjects.map(({ id, label }) => {
        const shares = found.map((game) => {
          const subject = game.subjects.find((one) => one.id === id);
          const raised = subject.mention_reviews;
          return {
            app_id: game.app_id,
            complaining: ratio(subject.criticised + subject.mixed, raised),
            mixed: ratio(subject.mixed, raised),
            praising: ratio(subject.praised + subject.mixed, raised),
            raised,
            rate: raised / game.reviews,
            reviews: game.reviews,
          };
        });
        const leaning = shares.filter((game) => game.raised >= 30);
        const rate = mean(shares.map((game) => game.rate));
        const spread =
          (1.96 *
            Math.sqrt(
              shares.reduce((acc, game) => acc + (game.rate * (1 - game.rate)) / game.reviews, 0),
            )) /
          shares.length;
        const sum = (pick) =>
          found.reduce(
            (total, game) => total + pick(game.subjects.find((one) => one.id === id)),
            0,
          );
        const raised = sum((one) => one.mention_reviews);
        const reviews = found.reduce((total, game) => total + game.reviews, 0);
        const pooled = raised / reviews;
        const pooledSpread = 1.96 * Math.sqrt((pooled * (1 - pooled)) / reviews);
        const [most] = shares.filter((game) => game.rate > 0).sort((a, b) => b.rate - a.rate);
        return {
          each_game: {
            complaining: mean(leaning.map((game) => game.complaining)),
            high: Math.min(rate + spread, 1),
            leaning_games: leaning.length,
            low: Math.max(rate - spread, 0),
            mixed: mean(leaning.map((game) => game.mixed)),
            praising: mean(leaning.map((game) => game.praising)),
            rate,
          },
          every_review: {
            complaining: ratio(
              sum((one) => one.criticised + one.mixed),
              raised,
            ),
            high: Math.min(pooled + pooledSpread, 1),
            leaning_games: shares.filter((game) => game.raised > 0).length,
            low: Math.max(pooled - pooledSpread, 0),
            mixed: ratio(
              sum((one) => one.mixed),
              raised,
            ),
            praising: ratio(
              sum((one) => one.praised + one.mixed),
              raised,
            ),
            rate: pooled,
          },
          games: shares,
          id,
          label,
          most: most?.app_id ?? null,
          raised_in: shares.filter((game) => game.rate >= 0.01).length,
        };
      });
      return {
        games: found.map((game) => ({ ...game, subjects: [] })),
        name,
        subjects: subjects ?? [],
        unread: group.app_ids
          .filter((appId) => !read.includes(appId))
          .map((appId) => ({
            app_id: appId,
            name: games.find((one) => one.app_id === appId)?.name ?? `App ${appId}`,
          })),
      };
    },
    groups: () => ({ groups }),
    induced: () => [],
    library: () => ({
      games: games.map((game) => ({
        app_id: game.app_id,
        coverage: game.coverage,
        name: game.name,
        reviews: game.reviews,
        snapshot: game.downloaded,
        stage: game.read ? 'read' : 'crawled',
        valve_total: game.valve_total,
        verdict: game.verdict,
      })),
      path: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
    }),
    look_up: ({ appId }) => ({
      app_id: appId,
      held: false,
      name: 'Delta',
      negative: 2000,
      positive: 10_000,
      reviews: 12_000,
      verdict: 'Very Positive',
    }),
    looked: () => null,
    meaning_offer: () => ({
      disk_bytes: 1_000_000,
      download_bytes: 0,
      every_game: false,
      job: null,
      on_card: true,
      recommended: 'prepare',
      seconds: 120,
      status: 'none',
    }),
    new_http_token: () => {
      httpToken = 'a1'.repeat(32);
      return answers.settings();
    },
    newer_version: () => (settings.check_newer_version ? newer : null),
    open_notices: () => null,
    open_report: () => null,
    overview: () => ({
      checked: whenLibrary(0, now - Day),
      claims: whenLibrary(0, 78_000),
      disk_bytes: whenLibrary(0, 22_400_000_000),
      games: games.length,
      jobs: board,
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      machine: {
        card: 'NVIDIA GeForce RTX 4090',
        card_bytes: 25_769_803_776,
        gpu_share: settings.gpu_share,
        models: [
          {
            bytes_left: 244_000_000,
            here: false,
            key: 'small',
            name: 'Small reader',
            newer: null,
            release: 'v1',
            role: 'faster, 78% agreement at 80% coverage',
            used: false,
          },
          {
            bytes_left: 0,
            here: true,
            key: 'standard',
            name: 'Standard reader',
            newer: 'v2',
            release: 'v1',
            role: '84% agreement at 80% coverage',
            used: true,
          },
          {
            bytes_left: 1_321_804_224,
            here: false,
            key: 'search-encoder',
            name: 'SteamGauge search encoder',
            newer: null,
            release: 'v1',
            role: 'finds what was said in other words',
            used: true,
          },
          {
            bytes_left: 2_664_279_746,
            here: false,
            key: 'writer',
            name: 'SteamGauge writer',
            newer: null,
            release: 'v1',
            role: 'writes what stands out about a game',
            used: false,
          },
        ].map((model) =>
          modelsSettled ? { ...model, bytes_left: 0, here: true, newer: null } : model,
        ),
        on_processor: false,
        reaches_card: true,
        reader: 'standard',
        releases_checked: now - Day,
      },
      moves: whenLibrary(
        [],
        [
          {
            app_id: 1,
            from: '2025-08',
            label: 'Performance',
            name: 'Alpha',
            shift: shift(0.12, 0.31, 9.1),
            side: 'complaint',
            since: '2024-08',
            subject: 'performance',
            to: '2025-10',
          },
        ],
      ),
      new_on_steam: whenLibrary([], [{ app_id: 1, name: 'Alpha', new: 1520 }]),
      not_read: whenLibrary([], [{ app_id: 3, name: 'Gamma' }]),
      older_reader: whenLibrary([], [{ app_id: 2, name: 'Beta' }]),
      read: whenLibrary(0, 2),
      recommended: whenLibrary(
        [],
        [
          {
            app_id: 1,
            from: '2025-08',
            name: 'Alpha',
            shift: shift(0.88, 0.8, -5.2),
            since: '2024-08',
            to: '2025-10',
          },
        ],
      ),
      reviews: whenLibrary(0, 25_900),
      since: games.length === 0 ? lately.first() : lately[latelyShown](),
    }),
    'plugin:opener|open_url': () => null,
    queue: ({ tasks }) => tasks.map(queued),
    queue_reads: ({ appIds }) =>
      appIds.map((appId) => queued({ app_id: appId, kind: 'read', language: 'english' })),
    queue_updates: ({ appIds }) =>
      (appIds ?? games.map((game) => game.app_id)).map((appId) =>
        queued({ app_id: appId, kind: 'update' }),
      ),
    read_offer: ({ appId }) => ({
      choices: [],
      download_bytes: appId === 3 ? 1_127_000_000 : 0,
      here: appId !== 3,
      published: true,
      reader: 'standard',
    }),
    reader_options: () => ({
      card: 'NVIDIA GeForce RTX 4090',
      card_bytes: 25_769_803_776,
      free_bytes: 182_000_000_000,
      on_processor: false,
      reaches_card: true,
      reads_with: settings.reader ?? 'standard',
      recommended: 'standard',
      sizes: [
        {
          agreement: 0.7814,
          bytes_left: 244_000_000,
          download_bytes: 244_000_000,
          name: 'small',
          needs: 1_278_214_144,
          published: true,
          runs_here: true,
          times: 1,
        },
        {
          agreement: 0.8443,
          bytes_left: fresh ? 1_127_000_000 : 0,
          download_bytes: 1_127_000_000,
          name: 'standard',
          needs: 4_294_967_296,
          published: true,
          runs_here: true,
          times: 6.2,
        },
      ],
    }),
    reading: ({ appId }) => reading(appId),
    remove_model: ({ key }) => {
      for (const model of models) {
        if (model.key === key) {
          model.bytes = 0;
        }
      }
      return storage();
    },
    save_groups: ({ groups: saved }) => {
      ({ groups } = saved);
      for (const game of games) {
        game.groups = groups
          .filter((one) => one.app_ids.includes(game.app_id))
          .map((one) => one.name);
      }
      return { groups };
    },
    save_settings: ({ settings: saved, searchEveryGame: every }) => {
      settings = saved;
      searchEveryGame = every;
      return answers.settings();
    },
    saw_what_changed: () => {
      if (updated) {
        updated.seen = true;
      }
      return null;
    },
    settings: () => ({
      ...settings,
      check_hours: [1, 6, 24, 168],
      clients,
      http: httpAnswer(),
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      mcp_command: `"${program}" mcp`,
      paste:
        'Add SteamGauge to yourself by following https://aureliolo.github.io/steamgauge/agent.md,\nthen tell me which file you changed.',
      search_every_game: searchEveryGame,
      shares: [0.25, 0.5, 0.75, 1],
    }),
    show_update_file: () => null,
    since_last_look: ({ appId }) => (sinceLastLook[appId] ?? (() => null))(),
    stop_job: ({ id }) => {
      board = board.map((job) =>
        job.id === id ? { ...job, note: 'Stopped.', state: 'stopped' } : job,
      );
      send('work', board);
    },
    storage: () => storage(),
    update_now: () => {
      update = { done: 0, left: null, rate: null, state: 'downloading', total: null };
      send('update', update);
      return null;
    },
    update_state: () => update,
    whats_new: () => {
      whatsNewAsked += 1;
      if (newer) {
        return { from: '0.1.0', releases, to: newer.version };
      }
      if (updated) {
        return { from: updated.from, releases: [], to: '0.1.0' };
      }
      return null;
    },
    who_wrote: (args) => whoWrote(args),
    work: () => board,
    writer_offer: () => ({
      download_bytes: writing ? 0 : 2_664_279_746,
      seconds: 60,
      write: writing,
    }),
  };

  globalThis.__TAURI__ = {
    core: {
      // The executor runs at once and turns a throw into a rejection, as an async command does.
      invoke: (command, args = {}) =>
        new Promise((resolve) => {
          calls.push({ args, command });
          const answer = answers[command];
          if (!answer) {
            throw new Error(`the stub has no answer for ${command}`);
          }
          resolve(answer(args));
        }),
    },
    event: {
      listen: (event, handler) => {
        listeners.set(event, [...(listeners.get(event) ?? []), handler]);
        return Promise.resolve(() => {
          // The stub keeps its listeners for the life of the page, so there is nothing to detach.
        });
      },
    },
  };

  // What the check reaches for: the calls made, and a way to move the board as the core would.
  globalThis.__stub = {
    board: (jobs) => {
      board = jobs;
      send('work', board);
    },
    calls,
    // What GitHub names when Check for updates asks next: a version, or null for out of reach.
    github: (version) => {
      github = version;
    },
    // A fresh answer, kept and then announced, in that order, as the core does.
    hear: (heard) => {
      newer = heard;
      send('newer-version', heard);
    },
    // The cockpit's first card in another state, announced as a finished job would be.
    lately: (state) => {
      latelyShown = state;
      send('library', null);
    },
    // Every question to GitHub so far failed.
    neverAnswered: () => {
      answeredAt = null;
    },
    refuse: (why) => {
      refusal = why;
    },
    send,
    settleModels: () => {
      modelsSettled = true;
      send('library', 1);
    },
    // Alpha's themes found again since its paragraph was written, announced as the job would.
    unwritten: () => {
      written = null;
      send('library', 1);
    },
    // The update moving on, as the core announces each step.
    update: (progress) => {
      update = progress;
      send('update', progress);
    },
    // This copy opening as a version newer than the one it last opened as.
    updatedFrom: (from) => {
      updated = { from, seen: false };
    },
    whatsNewAsked: () => whatsNewAsked,
  };
})();
