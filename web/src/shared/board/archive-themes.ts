import catalog from '../../generated-themes.json';
import type { BgBlitzTheme } from './themes';

// Generated from the validated local archives by tools/prepare_themes.py.
const archiveThemes: BgBlitzTheme[] = catalog;
export default archiveThemes;
