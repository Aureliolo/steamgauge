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

  // A gap between two kinds of reviewer, marked as the core marks one.
  const gap = (share, against, z) => ({
    share,
    against,
    z,
    clear: Math.abs(z) >= 4 && Math.abs(share - against) >= 0.02,
  });

  // The kinds of reviewer a read game was counted for, as the core lists them, and how a
  // sentence names each.
  const kind = (id, label, reviews, recommended) => ({ id, label, reviews, enough: reviews >= 100, recommended });
  const kinds = () => [
    {
      id: 'played',
      label: 'Time played when they wrote it',
      kinds: [
        kind('under-2-hours', 'Under 2 hours', 1_650, gap(0.62, 0.87, -14.2)),
        kind('2-to-10-hours', '2 to 10 hours', 3_450, gap(0.83, 0.86, -2.1)),
        kind('10-to-30-hours', '10 to 30 hours', 3_750, gap(0.88, 0.84, 3.0)),
        kind('30-to-100-hours', '30 to 100 hours', 3_450, gap(0.9, 0.84, 5.3)),
        kind('100-hours-or-more', '100 hours or more', 2_700, gap(0.91, 0.84, 6.1)),
      ],
    },
    {
      id: 'deck',
      label: 'Where they played',
      kinds: [kind('steam-deck', 'Mostly on a Steam Deck', 12, null), kind('elsewhere', 'Mostly elsewhere', 14_988, null)],
    },
    {
      id: 'early-access',
      label: 'When they wrote it',
      kinds: [
        kind('early-access', 'During early access', 4_500, gap(0.81, 0.87, -6.4)),
        kind('after-release', 'After release', 10_500, gap(0.87, 0.81, 6.4)),
      ],
    },
    {
      id: 'copy',
      label: 'How they got the game',
      kinds: [kind('got-it-free', 'Got it free', 600, gap(0.86, 0.85, 0.7)), kind('paid-for-it', 'Paid for it', 14_400, gap(0.85, 0.86, -0.7))],
    },
  ];
  const phrases = {
    'under-2-hours': 'Reviewers with under 2 hours played',
    '2-to-10-hours': 'Reviewers with 2 to 10 hours played',
    '10-to-30-hours': 'Reviewers with 10 to 30 hours played',
    '30-to-100-hours': 'Reviewers with 30 to 100 hours played',
    '100-hours-or-more': 'Reviewers with 100 hours or more played',
    'early-access': 'Reviewers writing during early access',
    'after-release': 'Reviewers writing after release',
    'got-it-free': 'Reviewers who got it free',
    'paid-for-it': 'Reviewers who paid for it',
  };
  const finding = (segment, label, subject, said, found, sentence) => ({ segment, label, subject, said, gap: found, sentence });
  const findings = () => [
    finding(
      '100-hours-or-more',
      '100 hours or more',
      'performance',
      'complains',
      gap(0.31, 0.12, 17.2),
      'Reviewers with 100 hours or more played complain about performance in 31.0% of their reviews, against 12.0% of everyone else.',
    ),
    finding(
      'under-2-hours',
      'Under 2 hours',
      null,
      'recommends',
      gap(0.62, 0.87, -14.2),
      'Reviewers with under 2 hours played recommend the game in 62.0% of their reviews, against 87.0% of everyone else.',
    ),
    finding(
      'under-2-hours',
      'Under 2 hours',
      'story',
      'praises',
      gap(0.21, 0.09, 8.1),
      'Reviewers with under 2 hours played praise story and writing in 21.0% of their reviews, against 9.0% of everyone else.',
    ),
    finding(
      'early-access',
      'During early access',
      'bugs',
      'complains',
      gap(0.18, 0.07, 7.7),
      'Reviewers writing during early access complain about bugs and crashes in 18.0% of their reviews, against 7.0% of reviewers writing after release.',
    ),
    finding(
      'early-access',
      'During early access',
      null,
      'recommends',
      gap(0.81, 0.87, -6.4),
      'Reviewers writing during early access recommend the game in 81.0% of their reviews, against 87.0% of reviewers writing after release.',
    ),
    finding(
      '30-to-100-hours',
      '30 to 100 hours',
      'content',
      'complains',
      gap(0.09, 0.05, 5.2),
      'Reviewers with 30 to 100 hours played complain about amount of content in 9.0% of their reviews, against 5.0% of everyone else.',
    ),
    finding(
      '2-to-10-hours',
      '2 to 10 hours',
      'tutorial',
      'praises',
      gap(0.06, 0.03, 4.4),
      'Reviewers with 2 to 10 hours played praise tutorial and learning in 6.0% of their reviews, against 3.0% of everyone else.',
    ),
  ];
  // One subject's figures among one side's reviews, with a band where its share would likely fall.
  const figures = (raised, reviews, praised, criticised, mixed) => ({
    raised,
    rate: raised / reviews,
    low: Math.max(0, raised / reviews - 0.025),
    high: Math.min(1, raised / reviews + 0.025),
    praised,
    criticised,
    mixed,
    praising: (praised + mixed) / reviews,
    complaining: (criticised + mixed) / reviews,
    claims: raised * 2,
  });
  const head = (id, label, who, reviews, recommending) => ({
    id,
    label,
    who,
    reviews,
    claims: reviews * 4,
    recommending,
    low: recommending - 0.02,
    high: recommending + 0.02,
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

  // What changed in a game since a look, as `since::Since` serialises it.
  const since = (fields) => ({
    looked: now - 2 * DAY,
    new: 0,
    read_new: 0,
    standing: 'nothing',
    from: null,
    to: null,
    recommended: null,
    moves: [],
    ...fields,
  });
  const alphaSince = () =>
    since({
      new: 1_520,
      read_new: 1_140,
      standing: 'compared',
      from: '2024-09',
      to: '2025-08',
      recommended: shift(0.86, 0.79, -5.6),
      moves: [
        { subject: 'performance', label: 'Performance', side: 'complaint', shift: shift(0.12, 0.24, 9.4) },
        { subject: 'story', label: 'Story and writing', side: 'praise', shift: shift(0.2, 0.14, -4.5) },
      ],
    });
  // The cockpit's first card in each of its states: things moved, nothing did, and a first look.
  const lately = {
    moved: () => ({
      looked: now - 2 * DAY,
      games: [
        { app_id: 1, name: 'Alpha', ...alphaSince() },
        { app_id: 2, name: 'Beta', ...since({ new: 260, standing: 'unread' }) },
        { app_id: 3, name: 'Gamma', ...since({ new: 35, standing: 'unread' }) },
      ],
    }),
    calm: () => ({ looked: now - 2 * DAY, games: [] }),
    first: () => ({ looked: null, games: [] }),
  };
  let latelyShown = 'moved';
  // Each game's own line: Alpha moved, Beta has nothing new, and Gamma's page was never seen.
  const sinceLastLook = { 1: alphaSince, 2: () => since({}), 3: () => null };

  // Opened with ?first, the core of somebody who has just installed the app: nothing downloaded.
  const first = new URLSearchParams(location.search).has('first');
  if (first) games.length = 0;

  let groups = [{ name: 'Rivals', app_ids: [2] }];
  let settings = {
    gpu_share: 0.5,
    reader: null,
    language: 'english',
    check_steam: true,
    keep_up_to_date: true,
    notify_moves: false,
    read_after_download: true,
    check_newer_version: true,
    answer_over_http: false,
    http_port: 47800,
  };
  // The token the HTTP server answers with, drawn again by New token.
  let httpToken = '0f'.repeat(32);
  // The answer the core kept from the last day's question.
  let newer = {
    version: '0.2.0',
    running: '0.1.0',
    url: 'https://github.com/Aureliolo/steamgauge/releases/tag/v0.2.0',
  };
  // Where Update now stands: nothing under way, on a copy the window can update.
  let update = { state: 'idle', why: null };
  let searchEveryGame = false;
  // What each game takes on disk, by part, and the models in the cache.
  let rooms = games.map((game, at) => ({
    app_id: game.app_id,
    name: game.name,
    parts: [
      { reviews: 2_400_000_000, reads: 1_100_000_000, search: 3_900_000_000, earlier: 1_800_000_000, partial: 120_000_000, other: 0 },
      { reviews: 640_000_000, reads: 300_000_000, search: 0, earlier: 0, partial: 0, other: 0 },
      { reviews: 110_000_000, reads: 0, search: 0, earlier: 0, partial: 40_000_000, other: 0 },
    ][at % 3],
  }));
  const models = [
    { name: 'Game Review Reader (small)', key: 'small', role: 'reads every review', bytes: 0, used: false },
    { name: 'Game Review Reader (standard)', key: 'standard', role: 'reads every review', bytes: 1_127_000_000, used: true },
    { name: 'SteamGauge search encoder', key: 'search-encoder', role: 'finds what was said in other words', bytes: 1_310_000_000, used: true },
    { name: 'SteamGauge search reranker', key: 'search-reranker', role: 'orders what it finds', bytes: 0, used: true },
  ];
  const storage = () => {
    const total = (parts) => Object.values(parts).reduce((sum, bytes) => sum + bytes, 0);
    const parts = { reviews: 0, reads: 0, search: 0, earlier: 0, partial: 0, other: 3_000_000 };
    for (const room of rooms) for (const key of Object.keys(room.parts)) parts[key] += room.parts[key];
    return {
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      models: 'C:\\Users\\someone\\AppData\\Local\\steamgauge\\models',
      drives: [{ name: 'C:', total: 1_000_000_000_000, free: 182_000_000_000, holds: ['library', 'models'] }],
      parts,
      games: rooms
        .map((room) => ({ ...room, parts: { ...room.parts }, total: total(room.parts) }))
        .sort((a, b) => b.total - a.total),
      model_rooms: models.map((model) => ({ ...model })),
    };
  };
  let board = [];
  let nextJob = 100;
  // What saving a game's data is refused with, or null where it is not.
  let refusal = null;
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
      months: calendar,
      languages: [{ name: 'english', reviews: 15_000, share: 0.75 }],
      // Beta was read before reviewers were told apart.
      who: appId === 1 ? { kinds: kinds(), findings: findings() } : { kinds: [], findings: [] },
    };
  };

  // One kind of reviewer beside everyone else or beside another kind, as `who_wrote` answers.
  const whoWrote = ({ these, others }) => {
    const all = kinds().flatMap((split) => split.kinds.map((one) => ({ ...one, split: split.id })));
    const first = all.find((one) => one.id === these);
    if (!first || !first.enough) throw new Error(`${first?.label ?? these}: too few reviews to count`);
    const second = others ? all.find((one) => one.id === others) : null;
    const rest = all
      .filter((one) => one.split === first.split && one.id !== first.id)
      .reduce((sum, one) => sum + one.reviews, 0);
    const against = second ? second.reviews : rest;
    // Counts written for everyone else, scaled to whoever the first kind is set beside.
    const scaled = (count) => Math.round((count * against) / 12_300);
    return {
      split: first.split,
      these: head(first.id, first.label, phrases[first.id], first.reviews, 0.91),
      others: second
        ? head(second.id, second.label, phrases[second.id], second.reviews, 0.62)
        : head('everyone-else', 'Everyone else', 'Everyone else', rest, 0.84),
      recommended: gap(0.91, 0.84, 6.1),
      subjects: [
        {
          id: 'performance',
          label: 'Performance',
          these: figures(1_000, first.reviews, 150, 700, 140),
          others: figures(scaled(2_900), against, scaled(700), scaled(1_700), scaled(160)),
          raised: gap(0.37, 0.24, 13.0),
          praise: gap(0.107, 0.07, 3.1),
          complaint: gap(0.31, 0.15, 17.2),
          these_corrected: null,
          others_corrected: null,
        },
        {
          id: 'story',
          label: 'Story and writing',
          these: figures(400, first.reviews, 300, 50, 20),
          others: figures(scaled(1_900), against, scaled(1_500), scaled(200), scaled(90)),
          raised: gap(0.15, 0.15, 0.2),
          praise: gap(0.12, 0.13, -0.9),
          complaint: gap(0.026, 0.023, 0.8),
          these_corrected: null,
          others_corrected: null,
        },
        {
          id: 'vr',
          label: 'VR and headsets',
          these: figures(0, first.reviews, 0, 0, 0),
          others: figures(0, against, 0, 0, 0),
          raised: gap(0, 0, 0),
          praise: gap(0, 0, 0),
          complaint: gap(0, 0, 0),
          these_corrected: null,
          others_corrected: null,
        },
      ],
      months: [
        { label: '2025-07', name: 'Jul 2025', reviews: 45, positive: 0.84 },
        { label: '2025-08', name: 'Aug 2025', reviews: 60, positive: 0.9 },
        { label: '2025-09', name: 'Sep 2025', reviews: 20, positive: null },
      ],
    };
  };

  // Fourteen months, September 2024 to October 2025, as a reading draws them.
  const calendar = Array.from({ length: 14 }, (_, at) => {
    const month = new Date(Date.UTC(2024, 8 + at, 15));
    return {
      label: `${month.getUTCFullYear()}-${String(month.getUTCMonth() + 1).padStart(2, '0')}`,
      name: month.toLocaleDateString('en-GB', { month: 'short', year: 'numeric', timeZone: 'UTC' }),
      reviews: 300 + ((at * 137) % 500),
      positive: 0.72 + ((at * 7) % 10) / 100,
    };
  });

  // Alpha's updates as the core sends them, each with its month and how far through it: one
  // before the calendar starts, one alone, three posted within days of each other, one with too
  // few reviews either side, and one the download holds only nine days after.
  const posting = (gid, title, year, month, day) => ({
    gid,
    title,
    posted: Date.UTC(year, month - 1, day, 12) / 1000,
    month: `${year}-${String(month).padStart(2, '0')}`,
    through: (day - 0.5) / new Date(Date.UTC(year, month, 0)).getUTCDate(),
    link: `https://store.steampowered.com/news/externalpost/steam_community_announcements/${gid}`,
  });
  const alphaUpdates = [
    posting('1840000000000000', 'Early Access Patch 0.9', 2024, 6, 1),
    posting('1840000000000001', 'Patch 1.1', 2024, 11, 12),
    posting('1840000000000002', 'The Winter Update 1.2', 2025, 1, 14),
    posting('1840000000000003', 'Hotfix 1.2.1', 2025, 1, 16),
    posting('1840000000000004', 'Hotfix 1.2.2', 2025, 1, 17),
    posting('1840000000000005', 'Patch 1.3', 2025, 6, 3),
    posting('1840000000000006', 'Patch 1.4: Performance and Stability', 2025, 9, 30),
  ];

  const compared = (before, after, change) => ({ before, after, z: change ? (after > before ? 4.2 : -4.2) : 0.6, change });
  const side = (subject, label, praise, complaint) => ({ subject, label, praise, complaint });
  const steady = [
    side('performance', 'Performance', compared(0.22, 0.23, false), compared(0.14, 0.13, false)),
    side('story', 'Story', compared(0.3, 0.31, false), compared(0.05, 0.05, false)),
  ];
  // What changed across each of Alpha's updates, by its id.
  const across = {
    '1840000000000002': {
      reviews: [1_840, 2_960],
      nearby: 2,
      recommended: compared(0.84, 0.71, true),
      subjects: [
        side('bugs', 'Bugs and crashes', compared(0.04, 0.03, false), compared(0.08, 0.19, true)),
        side('performance', 'Performance', compared(0.22, 0.31, true), compared(0.14, 0.13, false)),
        side('story', 'Story', compared(0.3, 0.31, false), compared(0.05, 0.05, false)),
      ],
    },
    '1840000000000003': { reviews: [1_900, 2_700], nearby: 2, recommended: compared(0.83, 0.72, true), subjects: steady },
    '1840000000000004': { reviews: [2_050, 2_400], nearby: 2, recommended: compared(0.81, 0.73, true), subjects: steady },
    '1840000000000001': { reviews: [980, 1_020], nearby: 0, recommended: compared(0.82, 0.83, false), subjects: steady },
    '1840000000000005': { reviews: [64, 41], nearby: 0, recommended: null, subjects: [] },
    '1840000000000006': { reviews: [1_200, 410], nearby: 0, recommended: compared(0.8, 0.79, false), subjects: steady, days: 9 },
  };
  // One kind of reviewer wrote a sixth of the reviews either side.
  const beforeAfter = (appId, gid, kind) => {
    const update = alphaUpdates.find((one) => one.gid === gid);
    const found = across[gid];
    if (appId !== 1 || !update || !found) throw new Error(`no update ${gid} is kept for app ${appId}`);
    const window = 28 * DAY;
    const after = found.days ? found.days * DAY : window;
    const reviews = kind ? found.reviews.map((count) => Math.round(count / 6)) : found.reviews;
    const enough = reviews[0] >= 100 && reviews[1] >= 100;
    return {
      update: { gid: update.gid, title: update.title, posted: update.posted, link: update.link },
      before: { from: update.posted - window, to: update.posted, reviews: reviews[0] },
      after: { from: update.posted, to: update.posted + after, reviews: reviews[1] },
      after_whole: !found.days,
      enough,
      recommended: found.recommended,
      subjects: found.subjects,
      changes: found.subjects.reduce((sum, one) => sum + one.praise.change + one.complaint.change, 0),
      nearby: found.nearby,
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
      background: false,
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
      read: games.length === 0 ? 0 : 2,
      reviews: games.length === 0 ? 0 : 25_900,
      claims: games.length === 0 ? 0 : 78_000,
      disk_bytes: games.length === 0 ? 0 : 22_400_000_000,
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      not_read: games.length === 0 ? [] : [{ app_id: 3, name: 'Gamma' }],
      older_reader: games.length === 0 ? [] : [{ app_id: 2, name: 'Beta' }],
      new_on_steam: games.length === 0 ? [] : [{ app_id: 1, name: 'Alpha', new: 1_520 }],
      checked: games.length === 0 ? 0 : now - DAY,
      moves: games.length === 0 ? [] : [
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
      recommended: games.length === 0 ? [] : [
        { app_id: 1, name: 'Alpha', from: '2025-08', to: '2025-10', since: '2024-08', shift: shift(0.88, 0.8, -5.2) },
      ],
      since: games.length === 0 ? lately.first() : lately[latelyShown](),
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
    // The core asks where in the system's dialog; a refusal stands in for a name already taken.
    export_data: ({ appId }) => {
      if (refusal) throw new Error(refusal);
      return queued({ kind: 'export_data', app_id: appId, to: 'C:\\Alpha - SteamGauge data' });
    },
    settings: () => ({
      ...settings,
      search_every_game: searchEveryGame,
      shares: [0.25, 0.5, 0.75, 1],
      library: 'C:\\Users\\someone\\AppData\\Local\\com.aureliolo.steamgauge\\data',
      claude_command: 'claude mcp add steamgauge -- "C:\\Users\\someone\\AppData\\Local\\SteamGauge\\steamgauge.exe" mcp',
      // Port 47801 stands in for one another program holds.
      http: !settings.answer_over_http
        ? { address: null, token: null, problem: null }
        : settings.http_port === 47801
          ? { address: null, token: null, problem: 'Nothing answers on port 47801: another program has it; choose another port' }
          : { address: `http://127.0.0.1:${settings.http_port}/mcp`, token: httpToken, problem: null },
    }),
    save_settings: ({ settings: saved, searchEveryGame: every }) => {
      settings = saved;
      searchEveryGame = every;
      return answers.settings();
    },
    new_http_token: () => {
      httpToken = 'a1'.repeat(32);
      return answers.settings();
    },
    reader_options: () => ({
      card: 'NVIDIA GeForce RTX 4090',
      card_bytes: 25_769_803_776,
      on_processor: false,
      reaches_card: true,
      recommended: 'standard',
      reads_with: settings.reader ?? 'standard',
      sizes: [
        { name: 'small', download_bytes: 244_000_000, bytes_left: 244_000_000, published: true, needs: 1_278_214_144, runs_here: true, times: 1 },
        { name: 'standard', download_bytes: 1_127_000_000, bytes_left: first ? 1_127_000_000 : 0, published: true, needs: 4_294_967_296, runs_here: true, times: 6.2 },
      ],
      free_bytes: 182_000_000_000,
    }),
    storage: () => storage(),
    free_room: ({ appId, what }) => {
      const parts = { search: ['search'], earlier: ['earlier'], partial: ['partial'], reads: ['reads'] }[what] ?? Object.keys(rooms[0].parts);
      for (const room of rooms) {
        if (appId !== null && room.app_id !== appId) continue;
        for (const part of parts) room.parts[part] = 0;
      }
      if (what === 'game') rooms = rooms.filter((room) => room.app_id !== appId);
      return storage();
    },
    remove_model: ({ key }) => {
      for (const model of models) if (model.key === key) model.bytes = 0;
      return storage();
    },
    newer_version: () => (settings.check_newer_version ? newer : null),
    update_state: () => update,
    update_now: () => {
      update = { state: 'downloading', done: 0, total: null, rate: null, left: null };
      send('update', update);
      return null;
    },
    show_update_file: () => null,
    'plugin:opener|open_url': () => null,
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
    looked: () => null,
    since_last_look: ({ appId }) => (sinceLastLook[appId] ?? (() => null))(),
    who_wrote: (args) => whoWrote(args),
    induced: () => [],
    // Alpha's developer posts updates; Beta was never asked about; anything else posted none.
    game_updates: ({ appId }) => ({
      asked: appId === 2 ? null : now - DAY,
      updates: appId === 1 ? alphaUpdates : [],
      window_days: 28,
      enough: 100,
    }),
    before_after: ({ appId, gid, kind }) => beforeAfter(appId, gid, kind),
    look_up: ({ appId }) => ({
      app_id: appId,
      name: 'Delta',
      reviews: 12_000,
      positive: 10_000,
      negative: 2_000,
      verdict: 'Very Positive',
      held: false,
    }),
    find_games: ({ words }) =>
      /delta/i.test(words)
        ? [
            { app_id: 4, name: 'Delta' },
            { app_id: 5, name: 'Delta: The Expansion' },
          ]
        : [],
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
    claims_behind: () => ({
      subject: 'performance',
      total: 2,
      from: 0,
      claims: [
        {
          claim: 'It runs smooth at 144 fps on a mid-range card.',
          review: 'Great combat. It runs smooth at 144 fps on a mid-range card. The story drags in act two.',
          language: 'english',
          polarity: 'praise',
          confidence: 0.94,
          voted_up: true,
          votes_up: 12,
          created: 1_756_000_000,
          url: 'https://steamcommunity.com/profiles/1/recommended/1/',
        },
        {
          claim: 'The frame rate stutters in every town.',
          review: 'The frame rate stutters in every town.',
          language: 'english',
          polarity: 'complaint',
          confidence: 0.88,
          voted_up: false,
          votes_up: 0,
          created: 1_757_000_000,
          url: 'https://steamcommunity.com/profiles/2/recommended/1/',
        },
      ],
    }),
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
    // A fresh answer, kept and then announced, in that order, as the core does.
    hear: (release) => {
      newer = release;
      send('newer-version', release);
    },
    // The cockpit's first card in another state, announced as a finished job would be.
    lately: (state) => {
      latelyShown = state;
      send('library', null);
    },
    // The update moving on, as the core announces each step.
    update: (progress) => {
      update = progress;
      send('update', progress);
    },
    refuse: (why) => {
      refusal = why;
    },
  };
})();
