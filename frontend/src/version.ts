/**
 * Videleum Frontend Version Information
 */

const VERSION = {
  number: "0.1.0",
  buildDate: new Date().toISOString().split("T")[0],
  name: "Videleum Frontend",
  displayVersion: function () {
    console.log(`
╔═══════════════════════════════════════════════╗
║                                               ║
║   ${this.name}                          ║
║   Version: ${this.number}                          ║
║   Build Date: ${this.buildDate}                    ║
║                                               ║
╚═══════════════════════════════════════════════╝
`);
  },
};

export default VERSION;
