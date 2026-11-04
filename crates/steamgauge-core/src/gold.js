// The adjudication page. Every question is in the file already and nothing is fetched to ask
// one. Answers are kept in this browser as they are made, because fourteen hundred claims is
// not one sitting and a closed tab must not cost a night's work.
//
// Opened from a file, the browser is the only copy until somebody presses Export, and a browser
// loses its storage for reasons nobody controls. Served by `steamgauge gold --serve`, every
// answer is posted as it is made and the disk is the copy that matters: the page reads the file
// back when it opens, the store here becomes a cache, and there is nothing to remember.
(() => {
  const data = JSON.parse(document.querySelector('#data').textContent);
  const questions = data.questions || [];
  const categories = data.categories || [];
  const app = document.querySelector('#app');
  // Per person as well as per draw: two people answering on one browser keep apart.
  const Store = `steamgauge-gold-${data.for}-${questions.length}-${data.taxonomy || ''}`;
  const Served = location.protocol === 'http:' || location.protocol === 'https:';

  const answers = load();
  let at = firstUnanswered();
  let notice = '';
  let kept = Served ? 'saved' : '';
  // When the question now on screen was put there, so an answer can record how long it took.
  let shownAt = Math.floor(Date.now() / 1000);
  let showing = -1;

  function load() {
    try {
      return JSON.parse(localStorage.getItem(Store) || '{}');
    } catch {
      return {};
    }
  }

  function save() {
    try {
      localStorage.setItem(Store, JSON.stringify(answers));
    } catch {
      /* A full or blocked store is not a reason to stop. The file on disk is the copy. */
    }
    if (Served) {
      post();
    }
  }

  // Posted whole rather than as a delta. A thousand answers is a few hundred kilobytes to a
  // process on the same machine, and sending the lot means a dropped request costs nothing: the
  // next answer carries everything the missed one did.
  //
  // One post at a time. A subject and a polarity are two keys and two posts, and two posts in
  // flight can land in either order, so the older one, with the polarity still missing, can
  // overwrite the newer on disk. A change made while one is out is sent when it returns, and
  // because a post carries everything, that one carries every change made meanwhile.
  let posting = false;
  let changedSince = false;

  function post() {
    if (posting) {
      changedSince = true;
      return;
    }
    posting = true;
    const rows = exportable();
    fetch('answers', {
      body: JSON.stringify(rows),
      headers: { 'Content-Type': 'application/json' },
      method: 'POST',
    })
      .then((reply) => {
        kept = reply.ok ? 'saved' : 'not saved';
      })
      .catch(() => {
        kept = 'not saved';
      })
      .then(() => {
        posting = false;
        if (changedSince) {
          changedSince = false;
          post();
          return;
        }
        paintKept();
      });
  }

  function paintKept() {
    const where = document.querySelector('#kept');
    if (where) {
      where.textContent = kept;
    }
  }

  // What is on disk wins over what this browser remembers: the file is the one copy that
  // survives a cleared cache and a second machine, and a browser holding a stale half of the
  // work must not overwrite it.
  function wantedKind(question) {
    // A re-judgement asks about a claim that is already answered on disk, and that answer is
    // the one being questioned: taking it as done would leave nothing to ask. Only an answer
    // marked as given with the rule in view counts.
    // An acceptability question is about one subject, and an answer about another is not its.
    if (question.offered) {
      return `offered:${question.offered}`;
    }
    return question.was ? 'rejudged' : 'any';
  }

  function fitsKind(kind, row) {
    if (kind === undefined) {
      return false;
    }
    if (kind === 'rejudged' && !row.rejudged) {
      return false;
    }
    return !(kind.indexOf('offered:') === 0 && kind !== `offered:${row.offered}`);
  }

  function adopt(rows) {
    const wanted = {};
    for (const question of questions) {
      wanted[keyOf(question)] = wantedKind(question);
    }
    let adopted = 0;
    for (const row of rows) {
      const key = `${row.app_id}#${row.review_id}#${row.index}`;
      if (fitsKind(wanted[key], row)) {
        answers[key] = row;
        adopted += 1;
      }
    }
    if (adopted) {
      at = firstUnanswered();
      render();
    }
  }

  // What the task is, in the page rather than in somebody's head. Without it the first screen
  // is a sentence, a review, twenty-six buttons and the word "polarity", and the commonest
  // thing to conclude from that is that the question is about the review. It is not: a review
  // is cut into its separate points and each one is its own question, so a review making
  // thirty-six points is thirty-six answers and not one.
  function howto() {
    return (
      '<details class="howto"' +
      (howtoOpen ? ' open' : '') +
      '><summary>What you are doing</summary>' +
      '<p>Each question is about <b>one claim</b>: a single point somebody made. The review it ' +
      'came from is shown underneath with the claim highlighted, because a claim like ' +
      '&ldquo;it doesn&rsquo;t&rdquo; or &ldquo;same here&rdquo; means nothing on its own. ' +
      '<b>Judge the highlighted claim, not the review around it.</b> A review that praises one ' +
      'thing and complains about thirty-four others is thirty-five separate claims, and you ' +
      'will be asked about them one at a time.</p>' +
      '<p>Two answers each time. <b>Which subject</b> the claim is about, from the sheet at the ' +
      'foot of the page, and <b>what it says about that subject</b>: praise, a complaint, or ' +
      'neither. Nothing else is being asked, so a claim naming no subject at all is ' +
      '<b>Overall verdict</b> when it judges the game and <b>Not about the game</b> when it ' +
      'does not.</p>' +
      '<p>Follow the sheet&rsquo;s rules where they disagree with your instinct: the rules are ' +
      'what make one person&rsquo;s answers comparable with another&rsquo;s, and where a rule ' +
      'is wrong that is worth finding out. If two subjects genuinely both fit and the sheet ' +
      'does not settle it, press <kbd>0</kbd>, which is not the same as being unsure ' +
      '(<kbd>8</kbd>).</p>' +
      '<p>You are not told which game it is, whether the reviewer recommended it, or what ' +
      'anyone else answered. That is deliberate: an answer on the page is an answer in your ' +
      'head, and a figure produced by agreeing with a suggestion measures nothing. Stopping ' +
      'part-way is fine and the work is kept.</p>' +
      '</details>'
    );
  }

  function howtoOffered() {
    return (
      '<details class="howto"' +
      (howtoOpen ? ' open' : '') +
      '><summary>What you are doing</summary>' +
      '<p>You have answered each of these claims before. The reader, the model that sorts ' +
      'claims into subjects, gave a different subject from yours. <b>Is its answer also fair ' +
      'for the highlighted claim?</b> A sentence often carries two subjects, and an answer you ' +
      'would not give yourself can still be one you would accept.</p>' +
      '<p>Your own answer stays exactly as it is whatever you say here. Press <kbd>y</kbd> ' +
      "if the reader's subject would also do and <kbd>n</kbd> if it is wrong. The rules for " +
      'both subjects are on the page.</p>' +
      '</details>'
    );
  }

  function keyOf(question) {
    return `${question.app_id}#${question.review_id}#${question.index}`;
  }

  // A subject without a polarity is a claim somebody is still on. It is kept so they can
  // finish it, and it is not an answer: counted as one it hid behind the count, and the page
  // stepped past it and never came back.
  function isAnswered(question) {
    const mine = answers[keyOf(question)];
    if (question.offered) {
      return Boolean(
        mine && mine.offered === question.offered && typeof mine.acceptable === 'boolean',
      );
    }
    return Boolean(mine?.subject && mine.polarity);
  }

  function labelOf(id) {
    const found = categories.find((one) => one.id === id);
    return found ? found.label : id;
  }

  // Whether a reader's other answer would also do. The person's own answer stands whatever they
  // say here. This asks what they would accept, which is a different thing from what they would
  // say, and human writing often fairly carries two subjects.
  function renderOffered(question, mine, html) {
    html.push(
      '<div class="shown"><span class="who">You said:</span><span><b>' +
        escapeHtml(labelOf(question.was.subject)) +
        '</b> ' +
        escapeHtml(question.was.polarity) +
        '</span><span class="who">The reader said:</span><span><b>' +
        escapeHtml(labelOf(question.offered)) +
        '</b></span></div>',
    );
    html.push('<div class="rules">');
    for (const category of categories) {
      if (category.id !== question.was.subject && category.id !== question.offered) {
        continue;
      }
      html.push(
        '<p><b>' +
          escapeHtml(category.label) +
          '</b>: ' +
          escapeHtml(category.description) +
          (category.boundary ? ` <i>${escapeHtml(category.boundary)}</i>` : '') +
          '</p>',
      );
    }
    html.push('</div>');
    html.push(
      '<div class="row last"><span class="label">Is <b>' +
        escapeHtml(labelOf(question.offered).toLowerCase()) +
        '</b> also an acceptable answer for this claim?</span>',
    );
    for (const [accepted, label, key] of [
      [true, 'yes, that is fair too', 'y'],
      [false, 'no, that is wrong', 'n'],
    ]) {
      html.push(
        '<button class="tone' +
          (mine.acceptable === accepted ? ' chosen' : '') +
          '" data-acceptable="' +
          accepted +
          '">' +
          label +
          ' <kbd>' +
          key +
          '</kbd></button>',
      );
    }
    html.push('</div>');
  }

  function firstUnanswered() {
    return nextUnanswered(0);
  }

  // The next question nobody has answered. The search wraps once, then gives up.
  //
  // An answered set is not a prefix. Claims are set aside and asked again when a rule moves
  // under them, a draw made later holds back what an earlier one asked, and either leaves
  // holes behind wherever the reader has got to. Stepping one place on from the last answer
  // walks straight into one of them, and the reader is shown a claim they have already
  // judged with no way to tell that from a claim they have not.
  function nextUnanswered(from) {
    for (let i = from; i < questions.length; i += 1) {
      if (!isAnswered(questions[i])) {
        return i;
      }
    }
    for (let j = 0; j < from && j < questions.length; j += 1) {
      if (!isAnswered(questions[j])) {
        return j;
      }
    }
    return questions.length;
  }

  function answered() {
    let n = 0;
    for (const question of questions) {
      if (isAnswered(question)) {
        n += 1;
      }
    }
    return n;
  }

  // Letters rather than numbers: twenty-six categories do not fit on the number row, and a
  // letter out of the category's own name is the one a reader remembers. Each category takes
  // the first letter of its name nobody has taken, then the first letter of its label, then
  // any letter at all, because two categories sharing a key means one of them cannot be
  // reached from the keyboard and the reader finds that out at claim four hundred.
  const Alphabet = 'abcdefghijklmnopqrstuvwxyz';
  const keys = {};
  const spent = {};

  // Whichever category has fewest of its own letters left goes first, rather than whichever
  // comes first in the taxonomy. Assigning in taxonomy order lets `accessibility` and
  // `atmosphere` spend the letters `story` and `policy` needed, and those two then take
  // whatever the alphabet has left, which is nothing anyone remembers.
  const own = (category) => `${category.id} ${category.label || ''}`.toLowerCase();
  const free = (category) => {
    let count = 0;
    const seen = {};
    const letters = own(category);
    for (const ch of letters) {
      if (Alphabet.indexOf(ch) >= 0 && !spent[ch] && !seen[ch]) {
        seen[ch] = true;
        count += 1;
      }
    }
    return count;
  };

  const waiting = categories.slice();
  while (waiting.length > 0) {
    waiting.sort((a, b) => free(a) - free(b));
    const category = waiting.shift();
    const tried = `${own(category)} ${Alphabet}`;
    for (const ch of tried) {
      if (Alphabet.indexOf(ch) >= 0 && !spent[ch]) {
        spent[ch] = true;
        keys[category.id] = ch;
        break;
      }
    }
  }

  function escapeHtml(text) {
    return String(text).replace(
      /[&<>"]/gu,
      (ch) => ({ '"': '&quot;', '&': '&amp;', '<': '&lt;', '>': '&gt;' })[ch],
    );
  }

  function marked(question) {
    // The claim is highlighted where it sits, rather than repeated: a reader asked to find the
    // sentence twice reads it once and guesses the second time. The review arrives already
    // split around the claim, so there is no offset here to be in the wrong units.
    return (
      escapeHtml(question.before || '') +
      '<mark>' +
      escapeHtml(question.claim || '') +
      '</mark>' +
      escapeHtml(question.after || '')
    );
  }

  // The page redraws itself on every answer, so anything the reader opened has to be put back
  // or the sheet slams shut the moment they use it.
  let sheetOpen = false;
  // Open until it is closed once, and closed thereafter: the explanation is needed on the
  // first question and in the way on the four hundredth.
  //
  // Deliberately not under `STORE`: that key holds the answers and only the answers, which is
  // an invariant the browser check asserts, and it is per-draw where this preference is not.
  const Howto = 'steamgauge-adjudication-howto';
  let howtoOpen = (() => {
    try {
      return localStorage.getItem(Howto) !== '0';
    } catch {
      return true;
    }
  })();

  function savedNote() {
    return Served
      ? 'Every answer is written to disk as you make it.'
      : 'Your answers are kept in this browser as you go.';
  }

  function renderBar(html) {
    html.push('<header class="bar">');
    html.push('<h1>Adjudicate</h1>');
    html.push(
      '<span class="count">' +
        (at + 1) +
        ' of ' +
        questions.length +
        ' &middot; ' +
        answered() +
        ' answered</span>',
    );
    html.push('<span class="spacer"></span>');
    if (Served) {
      html.push(`<span class="count" id="kept">${escapeHtml(kept)}</span>`);
    }
    html.push('<button class="go quiet" id="export">Export answers</button>');
    html.push(restoreControl());
    html.push('</header>');
    if (notice) {
      html.push(`<p class="note loaded">${escapeHtml(notice)}</p>`);
    }
    html.push(
      '<div class="progress"><i style="width:' +
        ((answered() / questions.length) * 100).toFixed(1) +
        '%"></i></div>',
    );
  }

  function renderShown(question, html) {
    html.push(
      '<div class="shown"><span class="who">' +
        (question.was ? 'The labellers said:' : 'Two labellers split:') +
        '</span>',
    );
    for (const [i, said] of question.shown.entries()) {
      html.push(
        take(question, said) +
          '<b>' +
          escapeHtml(said.subject) +
          '</b> ' +
          escapeHtml(said.polarity) +
          ' <span class="who">(' +
          escapeHtml(said.confidence) +
          (said.ambiguous ? ', called it contested' : '') +
          ')</span>' +
          (question.was ? '</button>' : '</span>'),
      );
      if (i === 0 && question.shown.length > 1) {
        html.push('<span class="who">vs</span>');
      }
    }
    html.push('</div>');
  }

  // A re-judgement asks a different question from a cold reading: not what the claim is
  // about, but whether the person accepts the rule the labellers applied. So the rule is
  // put in front of them, for every category anybody named, and their own first answer
  // beside it. The answer they give replaces the first.
  function renderWas(question, html) {
    html.push(
      '<div class="shown"><span class="who">You said:</span>' +
        take(question, question.was) +
        '<b>' +
        escapeHtml(question.was.subject) +
        '</b> ' +
        escapeHtml(question.was.polarity) +
        '</button></div>',
    );
    const named = [question.was.subject].concat((question.shown || []).map((said) => said.subject));
    html.push('<div class="rules">');
    for (const category of categories) {
      if (named.indexOf(category.id) === -1) {
        continue;
      }
      html.push(
        '<p><b>' +
          escapeHtml(category.label) +
          '</b>: ' +
          escapeHtml(category.description) +
          (category.boundary ? ` <i>${escapeHtml(category.boundary)}</i>` : '') +
          '</p>',
      );
    }
    html.push('</div>');
  }

  function renderGrid(mine, html) {
    html.push('<div class="grid">');
    for (const category of categories) {
      html.push(
        '<button class="pick' +
          (mine.subject === category.id ? ' chosen' : '') +
          '" data-subject="' +
          escapeHtml(category.id) +
          '" title="' +
          escapeHtml(category.description || '') +
          '"><kbd>' +
          escapeHtml(keys[category.id] || '') +
          '</kbd><span class="what">' +
          escapeHtml(category.label) +
          '</span></button>',
      );
    }
    html.push('</div>');
  }

  function renderFlags(mine, html) {
    // Before the polarity, not after it. Answering both halves moves the page on, so a row
    // below the polarity is a row the reader never reaches without going back for it.
    html.push('<div class="row"><span class="label">Anything else? (optional)</span>');
    html.push(
      '<button class="tone' +
        (mine.ambiguous ? ' chosen' : '') +
        '" id="ambiguous">two subjects both fit <kbd>0</kbd></button>',
    );
    html.push(
      '<button class="tone' +
        (mine.split_wrong ? ' chosen' : '') +
        '" id="split">this is two points, or half of one <kbd>9</kbd></button>',
    );
    // The reader's own uncertainty, kept apart from whether the claim is contested: one is
    // about them and one is about the sheet, and a set that conflates them cannot say which
    // of the two a disagreement came from.
    html.push(
      '<button class="tone' +
        (mine.unsure ? ' chosen' : '') +
        '" id="unsure">I am not sure <kbd>8</kbd></button>',
    );
    html.push('</div>');
  }

  function renderPolarity(mine, html) {
    const chosen = mine.subject ? categories.find((one) => one.id === mine.subject)?.label : null;
    html.push(
      '<div class="row last"><span class="label">Does the claim praise ' +
        (chosen ? `<b>${escapeHtml(chosen.toLowerCase())}</b>` : 'that') +
        ' or complain about it?</span>',
    );
    for (const [i, pair] of [
      ['praise', 'praises it'],
      ['complaint', 'complains about it'],
      ['neutral', 'neither'],
    ].entries()) {
      html.push(
        '<button class="tone' +
          (mine.polarity === pair[0] ? ' chosen' : '') +
          '" data-tone="' +
          pair[0] +
          '">' +
          pair[1] +
          ' <kbd>' +
          (i + 1) +
          '</kbd></button>',
      );
    }
    html.push('</div>');
    html.push('</div>');
  }

  function renderSheet(html) {
    html.push(
      '<p class="note"><kbd>&larr;</kbd> and <kbd>&rarr;</kbd> move, a letter picks a subject, ' +
        '<kbd>1</kbd>&ndash;<kbd>3</kbd> praise, complaint or neither. ' +
        'Pick both and it moves on by itself; flag it <em>after</em> answering and it waits ' +
        'for <kbd>&rarr;</kbd>.' +
        ' <kbd>0</kbd> two subjects fit, <kbd>9</kbd> cut wrong, <kbd>8</kbd> not sure. ' +
        savedNote() +
        '</p>',
    );

    html.push(
      '<details class="sheet"' +
        (sheetOpen ? ' open' : '') +
        '><summary>The category sheet</summary><dl>',
    );
    for (const category of categories) {
      html.push(
        '<dt>' +
          escapeHtml(category.label) +
          ' <kbd>' +
          escapeHtml(keys[category.id] || '') +
          '</kbd></dt>',
      );
      html.push(`<dd>${escapeHtml(category.description || '')}</dd>`);
      if (category.boundary) {
        html.push(`<dd><em>${escapeHtml(category.boundary)}</em></dd>`);
      }
    }
    html.push('</dl></details>');
  }

  function renderOfferedPage(question, mine, html) {
    renderOffered(question, mine, html);
    html.push('</div>');
    html.push(
      '<p class="note"><kbd>y</kbd> or <kbd>n</kbd> answers and moves on; <kbd>&larr;</kbd> ' +
        'and <kbd>&rarr;</kbd> move. ' +
        savedNote() +
        '</p>',
    );
  }

  function render() {
    if (at >= questions.length) {
      return renderDone();
    }
    // Only when the question changes: a redraw after each keypress would reset the clock and
    // every claim would look like it took as long as its last answer.
    if (at !== showing) {
      showing = at;
      shownAt = Math.floor(Date.now() / 1000);
    }
    const question = questions[at];
    const mine = answers[keyOf(question)] || {};

    const html = [];
    renderBar(html);

    html.push(question.offered ? howtoOffered() : howto());

    html.push('<div class="card">');
    html.push('<p class="asking">The claim you are judging</p>');
    html.push(`<p class="claim">${escapeHtml(question.claim)}</p>`);
    html.push('<p class="asking">The review it came from, so you can tell what it refers to</p>');
    html.push(`<div class="review">${marked(question)}</div>`);

    if (question.offered) {
      renderOfferedPage(question, mine, html);
      app.innerHTML = html.join('');
      wire(question);
      return;
    }

    if (question.shown && question.shown.length > 0) {
      renderShown(question, html);
    }
    if (question.was) {
      renderWas(question, html);
    }
    renderGrid(mine, html);
    renderFlags(mine, html);
    renderPolarity(mine, html);
    renderSheet(html);

    app.innerHTML = html.join('');
    wire(question);
  }

  function renderDone() {
    app.innerHTML =
      '<div class="done"><h2>' +
      answered() +
      ' of ' +
      questions.length +
      ' answered</h2>' +
      '<p class="note">' +
      (Served
        ? 'Already on disk. Run <code>steamgauge ingest-gold</code> on it.'
        : 'Export the file and run <code>steamgauge ingest-gold</code> on it.') +
      '</p>' +
      '<p><button class="go" id="export">Export answers</button> ' +
      restoreControl() +
      ' <button class="go quiet" id="back">Back to the last one</button></p>' +
      (notice ? `<p class="note loaded">${escapeHtml(notice)}</p>` : '') +
      '</div>';
    document.querySelector('#export').onclick = exportAnswers;
    wireRestore();
    document.querySelector('#back').onclick = () => {
      at = Math.max(0, questions.length - 1);
      render();
    };
  }

  function set(question, field, value) {
    const key = keyOf(question);
    const mine = answers[key] || {
      answered_at: 0,
      app_id: question.app_id,
      index: question.index,
      review_id: question.review_id,
      // When the question was first put in front of somebody and when they last touched it.
      // Two claims a second is somebody clicking through rather than reading, and a rate is
      // also the only honest way to cost this work before asking anybody to be paid for it.
      shown_at: shownAt,
    };
    mine[field] = value;
    if (question.offered) {
      mine.offered = question.offered;
      mine.answered_at = Math.floor(Date.now() / 1000);
      answers[key] = mine;
      save();
      at = nextUnanswered(at + 1);
      render();
      return;
    }
    // Given with the labellers' answers and the sheet's rule in view, which is a different
    // kind of answer from a cold one and is filed as such.
    if (question.was) {
      mine.rejudged = true;
    }
    mine.answered_at = Math.floor(Date.now() / 1000);
    answers[key] = mine;
    save();
    // Moving on the moment both halves of an answer exist is what makes fourteen hundred
    // claims possible: the reader never touches a "next" button.
    //
    // What holds the page is a flag pressed last, not a flag existing. Somebody who decides a
    // claim is contested after answering it would otherwise watch the page leave while they
    // reached for the key, and those are the claims the whole exercise is for. Flagging first
    // and then answering is the ordinary way round, and it has no reason to wait.
    const flag = field === 'ambiguous' || field === 'split_wrong' || field === 'unsure';
    if (mine.subject && mine.polarity && !flag) {
      at = nextUnanswered(at + 1);
    }
    render();
  }

  // On a re-judgement the answer is always one already on the page, so each of them is a
  // button that gives both halves at once. On a split it is not: an answer shown there is a
  // suggestion, and a suggestion with a button on it is a suggestion taken.
  function take(question, said) {
    if (!question.was) {
      return '<span>';
    }
    return (
      '<button class="take" data-subject="' +
      escapeHtml(said.subject) +
      '" data-polarity="' +
      escapeHtml(said.polarity) +
      '">'
    );
  }

  function wire(question) {
    Array.prototype.forEach.call(app.querySelectorAll('button.pick'), (button) => {
      button.onclick = () => {
        set(question, 'subject', button.getAttribute('data-subject'));
      };
    });
    Array.prototype.forEach.call(app.querySelectorAll('button.take'), (button) => {
      button.onclick = () => {
        set(question, 'subject', button.getAttribute('data-subject'));
        set(question, 'polarity', button.getAttribute('data-polarity'));
      };
    });
    Array.prototype.forEach.call(app.querySelectorAll('button.tone[data-tone]'), (button) => {
      button.onclick = () => {
        set(question, 'polarity', button.getAttribute('data-tone'));
      };
    });
    Array.prototype.forEach.call(app.querySelectorAll('button.tone[data-acceptable]'), (button) => {
      button.onclick = () => {
        set(question, 'acceptable', button.getAttribute('data-acceptable') === 'true');
      };
    });
    // A long review opens at its top and the claim can be anywhere in it, so without this the
    // reader hunts for the highlight before they can start. Centred rather than scrolled just
    // into view, because a claim resting against the bottom edge of the box is read with no
    // idea what follows it, and what follows it is half of why the review is here at all.
    const highlight = app.querySelector('.review mark');
    if (highlight) {
      const box = highlight.parentNode;
      // Measured as rectangles rather than offsets: `offsetTop` is relative to the nearest
      // positioned ancestor, the review box is not one, and the difference is the whole page.
      const seen = highlight.getBoundingClientRect();
      const frame = box.getBoundingClientRect();
      box.scrollTop = Math.max(
        0,
        box.scrollTop + (seen.top - frame.top) - box.clientHeight / 2 + seen.height / 2,
      );
    }
    const mine = answers[keyOf(question)] || {};
    const ambiguous = document.querySelector('#ambiguous');
    if (ambiguous) {
      ambiguous.onclick = () => {
        set(question, 'ambiguous', !mine.ambiguous);
      };
    }
    const split = document.querySelector('#split');
    if (split) {
      split.onclick = () => {
        set(question, 'split_wrong', !mine.split_wrong);
      };
    }
    const unsure = document.querySelector('#unsure');
    if (unsure) {
      unsure.onclick = () => {
        set(question, 'unsure', !mine.unsure);
      };
    }
    const out = document.querySelector('#export');
    if (out) {
      out.onclick = exportAnswers;
    }
    wireRestore();
    const sheet = app.querySelector('details.sheet');
    if (sheet) {
      sheet.ontoggle = () => {
        sheetOpen = sheet.open;
      };
    }
    const how = app.querySelector('details.howto');
    if (how) {
      how.ontoggle = () => {
        howtoOpen = how.open;
        try {
          localStorage.setItem(Howto, howtoOpen ? '1' : '0');
        } catch {
          /* Closing the instructions is a convenience, not work to be kept. */
        }
      };
    }
  }

  // The browser is the only copy of the work until the file is exported, and a browser loses
  // its storage for reasons nobody controls: a cleared cache, a private window, another
  // machine. Reading an exported file back turns the export into a save rather than a delivery.
  function restoreControl() {
    return (
      '<label class="go quiet" for="restore">Load answers</label>' +
      '<input type="file" id="restore" accept="application/json,.json" hidden>'
    );
  }

  function wireRestore() {
    const input = document.querySelector('#restore');
    if (!input) {
      return;
    }
    input.onchange = () => {
      const file = input.files?.[0];
      if (file) {
        restore(file);
      }
    };
  }

  function holdsAnswer(row) {
    return Boolean(row) && Boolean(row.subject || typeof row.acceptable === 'boolean');
  }

  function mergeRows(rows) {
    const wanted = {};
    for (const question of questions) {
      wanted[keyOf(question)] = true;
    }
    let loaded = 0;
    let strangers = 0;
    let others = 0;
    for (const row of rows) {
      if (!holdsAnswer(row)) {
        continue;
      }
      // Loaded here they would be exported again under this page's name, one person's
      // answers filed as another's.
      if (row.by && row.by !== data.for) {
        others += 1;
        continue;
      }
      const key = `${row.app_id}#${row.review_id}#${row.index}`;
      // A file from another draw would fill the store with answers to questions this page
      // never asks, and the count in the corner would climb while nothing got adjudicated.
      if (!wanted[key]) {
        strangers += 1;
        continue;
      }
      answers[key] = row;
      loaded += 1;
    }
    return { loaded, others, strangers };
  }

  function restore(file) {
    const reader = new FileReader();
    reader.onload = () => {
      let rows;
      try {
        rows = JSON.parse(String(reader.result));
      } catch {
        rows = null;
      }
      if (!(rows && rows.length > 0)) {
        notice = 'that file holds no answers';
        return render();
      }
      const { loaded, others, strangers } = mergeRows(rows);
      save();
      at = firstUnanswered();
      notice =
        loaded +
        ' answers loaded' +
        (strangers ? `, ${strangers} from another draw ignored` : '') +
        (others ? `, ${others} given by someone else ignored` : '');
      render();
    };
    reader.readAsText(file);
  }

  function exportable() {
    const rows = [];
    for (const question of questions) {
      const mine = answers[keyOf(question)];
      if (!mine || (!mine.subject && typeof mine.acceptable !== 'boolean')) {
        continue;
      }
      // Stamped per answer rather than once per file, because an answer outlives the file it
      // was exported in: these get merged, re-exported and ingested separately. A gold label
      // whose sheet nobody can name is the one label here that cannot be checked against
      // anything, and an unnamed one already cost 39 answers.
      mine.sheet = data.taxonomy;
      mine.by = data.for;
      rows.push(mine);
    }
    return rows;
  }

  // The export is otherwise unreachable from outside, and a stamp nothing can inspect is a
  // stamp nobody will notice the absence of.
  globalThis.__exportForCheck = exportable;

  function exportAnswers() {
    const blob = new Blob([JSON.stringify(exportable(), null, 2)], { type: 'application/json' });
    const link = document.createElement('a');
    link.href = URL.createObjectURL(blob);
    link.download = `gold-answers-${data.for}.json`;
    document.body.appendChild(link);
    link.click();
    document.body.removeChild(link);
    URL.revokeObjectURL(link.href);
  }

  const polarityKeys = new Map([
    ['1', 'praise'],
    ['2', 'complaint'],
    ['3', 'neutral'],
  ]);
  const flagKeys = new Map([
    ['0', 'ambiguous'],
    ['9', 'split_wrong'],
    ['8', 'unsure'],
  ]);

  function answerKey(question, key) {
    if (question.offered) {
      // Only yes and no mean anything here: a letter that picks a subject elsewhere on the page
      // would otherwise answer a question nobody is asking.
      if (key === 'y' || key === 'n') {
        set(question, 'acceptable', key === 'y');
      }
    } else if (polarityKeys.has(key)) {
      set(question, 'polarity', polarityKeys.get(key));
    } else if (flagKeys.has(key)) {
      const field = flagKeys.get(key);
      const mine = answers[keyOf(question)] || {};
      set(question, field, !mine[field]);
    } else {
      const found = Object.entries(keys).find(([, letter]) => letter === key);
      if (found) {
        set(question, 'subject', found[0]);
      }
    }
  }

  document.addEventListener('keydown', (event) => {
    if (event.metaKey || event.ctrlKey || event.altKey) {
      return;
    }
    if (at >= questions.length) {
      return;
    }
    if (event.key === 'ArrowRight') {
      at = Math.min(questions.length, at + 1);
      render();
    } else if (event.key === 'ArrowLeft') {
      at = Math.max(0, at - 1);
      render();
    } else {
      answerKey(questions[at], event.key);
    }
  });

  render();

  if (Served) {
    fetch('answers')
      .then((reply) => (reply.ok ? reply.json() : []))
      .then((rows) => {
        if (rows && rows.length > 0) {
          adopt(rows);
        }
      })
      .catch(() => {
        kept = 'not saved';
        paintKept();
      });
  }
})();
