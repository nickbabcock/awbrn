export default {
  journal: {
    entries: [{ idx: 0, when: 1775098334477, tag: "0000_cold_ezekiel", breakpoints: true }],
  },
  migrations: {
    m0000: `CREATE TABLE \`events\` (
\t\`seq\` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
\t\`kind\` text NOT NULL,
\t\`payload\` text NOT NULL,
\t\`createdAt\` integer NOT NULL
);`,
  },
};
