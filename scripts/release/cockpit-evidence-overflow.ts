export type OverflowSnapshot = {
  fits?: unknown;
  viewportWidth?: unknown;
  documentScrollWidth?: unknown;
  offenders?: unknown;
};

/** Runs in the candidate browser, so failure details reflect rendered layout. */
export const horizontalOverflowSnapshotExpression = `(()=>{
  const viewportWidth=window.innerWidth;
  const describe=(element)=>{
    const rect=element.getBoundingClientRect();
    const classes=Array.from(element.classList).slice(0,4);
    const selector=element.id?\`#\${CSS.escape(element.id)}\`:element.hasAttribute('data-evidence-issue')?\`[data-evidence-issue=\"\${element.getAttribute('data-evidence-issue')}\"]\`:\`\${element.tagName.toLowerCase()}\${classes.map((name)=>\`.\${CSS.escape(name)}\`).join('')}\`;
    return {selector,tag:element.tagName.toLowerCase(),classes,rect:{left:Math.round(rect.left*10)/10,right:Math.round(rect.right*10)/10,width:Math.round(rect.width*10)/10},scrollWidth:element.scrollWidth,clientWidth:element.clientWidth};
  };
  const offenders=Array.from(document.querySelectorAll('*')).filter((element)=>{
    const rect=element.getBoundingClientRect();
    return rect.right>viewportWidth+.5||rect.left<-.5||element.scrollWidth>element.clientWidth+1;
  }).map(describe).sort((a,b)=>Math.max(b.rect.right-viewportWidth,b.scrollWidth-b.clientWidth)-Math.max(a.rect.right-viewportWidth,a.scrollWidth-a.clientWidth)).slice(0,12);
  return {fits:document.documentElement.scrollWidth<=viewportWidth,viewportWidth,documentScrollWidth:document.documentElement.scrollWidth,offenders};
})()`;

export function horizontalOverflowFailure(scenario: string, snapshot: OverflowSnapshot | undefined): string {
  return `${scenario} has no horizontal overflow; browser observed ${JSON.stringify({
    viewportWidth: snapshot?.viewportWidth,
    documentScrollWidth: snapshot?.documentScrollWidth,
    offenders: snapshot?.offenders,
  })}`;
}
