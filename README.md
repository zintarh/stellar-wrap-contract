# Stellar Wrap 

> **Turn your ledger data into social proof. A shareable, weekly/monthly/yearly summary of your impact on the Stellar network.**

---

## 📖 What is Stellar Wrap?

Stellar Wrap is a "Spotify Wrapped"-style experience built specifically for the Stellar community.

Block explorers are great for data, but terrible for stories. Stellar Wrap takes your raw, complex on-chain history—transactions, smart contract deployments, NFT buys—and transforms it into a beautiful, personalized visual story that anyone can understand and share.

By simply connecting your wallet, you get a dynamic snapshot of your month on Stellar, highlighting your achievements and assigning you a unique on-chain persona based on your activity.

**It's more than just stats; it's a tool for builders to prove their contributions and for users to flex their participation in the Stellar ecosystem.**

---

## 💡 Why We Need This

In Web3, your on-chain history is your resume, your identity, and your reputation. But right now, that reputation is hidden behind confusing transaction hashes.

**Stellar Wrap solves the visibility gap:**

* **For Builders & Developers:** It's hard to showcase the immense value of deploying open-source Soroban contracts. Stellar Wrap makes their code contributions visible and shareable to non-technical users.
* **For the Community:** We lack easy, viral loops to share excitement about what's happening on Stellar. This tool gives everyone a reason to post about their on-chain life on social media.
* **For Users:** It turns isolated transactions into a sense of progress and belonging within the ecosystem.

---

## 🚀 How It Works

1.  **Connect:** Connect your Stellar wallet (e.g., Freighter, xBull) to our web app.
2.  **Analyze:** Our backend crunches your on-chain history for the month, pulling data on payments, DEX trades, Soroban interactions, and NFTs.
3.  **Visualize:** The frontend presents this data as a slick, animated story, highlighting your key stats.
4.  **Persona:** Based on your specific behavior, you get assigned a fun archetype (e.g., *"The Soroban Architect," "The DeFi Patron," "The Diamond Hand"*).
5.  **Share:** Generate a beautiful, branded image card ready for one-click sharing to X (Twitter), Farcaster, etc.

---

## 🎯 Key Metrics Tracked

We look beyond simple payments to capture the full spectrum of Stellar's vibrant ecosystem:

* **🧙‍♂️ Soroban Builder Stats:** Contracts deployed and unique user interactions. (Critical for developer reputation!).
* **🤝 dApp Interactions:** Which ecosystem projects did you support the most?
* **🎨 NFT Activity:** New mints collected and top creators supported.
* **💸 Network Volume:** A summary of your general transaction activity.
* **🏆 Your Monthly Persona:** A gamified badge that reflects your unique contribution style.

---

## Ecosystem Impact

This project is designed to support the growth of the Stellar network by:

1.  **Incentivizing Building:** Publicly celebrating developers who ship code creates positive reinforcement. A "Soroban Architect" badge is a social flex that encourages more building.
2.  **Driving Viral Activity:** Every shared Stellar Wrap card is organic marketing for the blockchain, showing the world that Stellar is active and being used.
3.  **Increasing Retention:** Giving users a personalized summary fosters a sense of ownership and encourages them to come back next month to beat their stats.

---

## 🛠️ Tech Stack

* **Frontend:** Next.js, React, TailwindCSS
* **Animations:** Framer Motion
* **Wallet Connection:** Stellar SDK, Freighter integration
* **Image Generation:** `satori` / `html2canvas` for creating shareable social cards.

---

## 🗺️ Roadmap

Our immediate focus is on delivering a polished MVP for the community:

* ✅ Seamless wallet integration (Freighter/Albedo).
* ✅ Core data fetching and aggregation logic for a 30-day period.
* ✅ Developing the persona assignment algorithm.
* ✅ Building the dynamic social media card generator.
* ✅ Live public release for community testing.
