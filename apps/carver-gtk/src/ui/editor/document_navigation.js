(() => {
    const session = __SESSION__;
    const revision = __LOAD__;
    let navigation_epoch = 0;
    const headings = () => [...document.querySelectorAll('h1,h2,h3,h4,h5,h6')];
    const pathFor = node => (node.getAttribute('src') ?? node.getAttribute('href') ?? '')
        .replace(/^carver-asset:\/\/\//, '');
    const mediaFor = path => [...document.querySelectorAll('img,a[href]')]
        .filter(node => pathFor(node) === path && (node.tagName === 'IMG' || path.startsWith('assets/')));
    const report = event => {
        const node = event.target.closest?.('img,a[href]');
        const path = node ? pathFor(node) : '';
        if (node?.tagName === 'A' && path.startsWith('assets/')) event.preventDefault();
        const heading = event.target.closest?.('h1,h2,h3,h4,h5,h6');
        const headingIndex = heading ? headings().indexOf(heading) : -1;
        const media = node && (node.tagName === 'IMG' || path.startsWith('assets/'))
            ? {path, occurrence: mediaFor(path).indexOf(node)} : null;
        window.webkit.messageHandlers.documentSelection.postMessage(JSON.stringify({
            type: 'selection', session,
            state: {active: [], heading: 0, revision, navigation_epoch, image_width: null, media,
                heading_occurrence: headingIndex < 0 ? null : headingIndex}
        }));
    };
    document.addEventListener('click', report);
    document.addEventListener('focusin', report);
    window.carverDocumentNavigation = {
        focus(target, expectedRevision, focus, epoch) {
            if (expectedRevision !== revision) return false;
            navigation_epoch = epoch;
            const node = target.kind === 'heading' ? headings()[target.occurrence]
                : mediaFor(target.path)[target.occurrence];
            if (!node) return false;
            if (focus) {
                node.setAttribute('tabindex', '-1');
                node.focus({preventScroll: true});
            }
            node.scrollIntoView({block: 'center'});
            return true;
        }
    };
})();
