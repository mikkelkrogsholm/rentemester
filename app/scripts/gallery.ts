import gallery from '../gallery.html';
Bun.serve({ port: 5320, development: true, routes: { '/*': gallery } });
