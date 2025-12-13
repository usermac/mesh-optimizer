# Mesh Optimizer Pricing Strategy

**Version:** 1.0  
**Last Updated:** December 2024  
**Status:** Pre-Launch Recommendations

---

## Executive Summary

This document outlines the recommended pricing strategy for the Mesh Optimizer service. The current flat-rate model ($50 for 100 credits) should be expanded into a **4-tier system** that captures customers across the value spectrum while maximizing lifetime value and reducing barriers to entry.

**Key Recommendation:** Launch with a **freemium model** to drive adoption, then implement tiered pricing within 30 days of launch.

---

## 1. Current Pricing Analysis

### Existing Model
```
CREDIT_COST = $50
CREDIT_INCREMENT = 100 credits
Effective Rate = $0.50 per credit
```

### Problems with Current Model
1. **High Barrier to Entry:** $50 minimum purchase deters trial customers
2. **No Volume Incentive:** Linear pricing doesn't reward loyal/high-volume users
3. **Single Tier:** Can't capture both freelancers ($25/month) and enterprises ($5,000/month)
4. **No Differentiation:** Doesn't communicate value tiers to market

### Strengths to Preserve
1. ✅ Credit system allows flexible consumption
2. ✅ No subscription lock-in (pay-as-you-go)
3. ✅ Simple to understand ($0.50/credit)

---

## 2. Recommended Pricing Tiers

### Overview Table

| Tier | Price | Credits | Cost/Credit | Discount | Target Customer |
|------|-------|---------|-------------|----------|-----------------|
| **Free** | $0 | 25 | $0 | - | Trial users, students |
| **Starter** | $25 | 50 | $0.50 | 0% | Freelancers, hobbyists |
| **Professional** | $50 | 110 | $0.45 | 10% | Small studios, agencies |
| **Studio** | $200 | 500 | $0.40 | 20% | Mid-size studios |
| **Enterprise** | Custom | Custom | $0.30-$0.35 | 30-40% | Large studios, platforms |

---

## 3. Detailed Tier Breakdown

### 🆓 Free Tier
**Price:** $0  
**Credits:** 25 (one-time)  
**Typical Usage:** 2-5 files optimized

**Purpose:**
- Eliminate friction for first-time users
- Allow complete workflow testing
- Build email list for nurture campaigns
- Reduce support burden ("try before you ask")

**Restrictions:**
- One-time only (per email/account)
- No credit card required
- Standard processing priority (lowest)
- 7-day result retention

**Conversion Strategy:**
```
Day 0: Welcome email + quick start guide
Day 2: "Here's what you can do with credits" (use case examples)
Day 5: "You have 15 credits left" (urgency)
Day 7: "20% off your first purchase" (offer expires in 48h)
Day 10: Case study email (social proof)
```

**Expected Conversion:** 15-25% to paid tier within 30 days

---

### 🌱 Starter Tier
**Price:** $25  
**Credits:** 50  
**Cost per Credit:** $0.50  
**Typical Usage:** 5-10 files optimized

**Target Customer:**
- Freelance 3D artists
- Independent game developers
- Students/educators
- Small e-commerce businesses

**Value Proposition:**
- "Get started for less than a coffee subscription"
- Perfect for occasional optimization needs
- Test API integration without major commitment

**Marketing Messaging:**
> "Optimize up to 10 models for just $25. No subscription, no commitment. Perfect for freelancers and small projects."

**Expected Volume:** 40-50% of paid customers

---

### 💼 Professional Tier (RECOMMENDED)
**Price:** $50  
**Credits:** 110 (10% bonus)  
**Cost per Credit:** $0.45  
**Typical Usage:** 10-22 files optimized

**Target Customer:**
- Small game studios (2-10 people)
- AR/VR agencies
- Architectural visualization firms
- Marketing agencies with 3D needs

**Value Proposition:**
- **10% more credits** vs Starter (psychological anchor)
- "Most Popular" badge on pricing page
- Ideal balance of cost and capacity

**Marketing Messaging:**
> "Our most popular plan. Get 110 credits for $50 — that's 10 free bonus credits. Enough to optimize 20+ models with room to experiment."

**Expected Volume:** 35-45% of paid customers

---

### 🏢 Studio Tier
**Price:** $200  
**Credits:** 500 (25% bonus = 400 + 100 free)  
**Cost per Credit:** $0.40  
**Typical Usage:** 50-100 files optimized

**Target Customer:**
- Mid-size game studios (10-50 people)
- 3D asset marketplace vendors
- Product visualization platforms
- Architecture firms with recurring needs

**Value Proposition:**
- **20% discount** on per-credit cost
- "Best Value" badge on pricing page
- Priority processing queue (+15% faster)
- 30-day result retention (vs 7 days)

**Additional Benefits:**
- Priority email support (24h response SLA)
- Monthly usage reports
- API rate limit: 20 requests/minute (vs 10)

**Marketing Messaging:**
> "Best value for studios. Save 20% with 500 credits for $200. Priority processing, extended retention, and dedicated support."

**Expected Volume:** 15-20% of paid customers

---

### 🏆 Enterprise Tier
**Price:** Custom ($1,500 - $5,000/month)  
**Credits:** Custom (3,750 - 12,500/month)  
**Cost per Credit:** $0.30 - $0.40 (negotiable)

**Target Customer:**
- AAA game studios
- E-commerce platforms (Shopify, Amazon 3D)
- AR/VR platforms (Meta, Apple ecosystem vendors)
- 3D scanning services
- Automotive/Industrial design firms

**Value Proposition:**
- **30-40% discount** on per-credit cost
- Dedicated processing infrastructure
- White-label API option
- Custom integrations
- SLA guarantees (99.5% uptime)
- Dedicated account manager

**Additional Benefits:**
- Highest priority queue
- Unlimited result retention
- Custom optimization presets
- Webhook notifications
- SSO/SAML support
- On-premise deployment option (future)

**Minimum Commitment:** 6 months

**Marketing Messaging:**
> "Enterprise-grade 3D optimization at scale. Custom pricing, dedicated infrastructure, and white-label options. Let's talk about your needs."

**Expected Volume:** 5-10% of customers, 40-60% of revenue

---

## 4. Credit Consumption Guide

### Estimated Credit Costs per Job Type

To help customers choose the right tier, publish this guide:

| Optimization Type | Credits | Example |
|-------------------|---------|---------|
| **Simple Decimation** | 10-15 | Reduce 100K to 10K polygons |
| **Medium Optimization** | 15-25 | Decimation + UV optimization |
| **Complex Remesh** | 25-40 | QuadriFlow + Normal baking |
| **Full Pipeline** | 40-60 | Remesh + Normal + Diffuse baking |
| **Large File (5GB)** | 60-80 | Complex FBX with textures |

**Calculator Tool:**
Create an interactive pricing calculator on the website:
```
"How many files do you need to optimize?"
[Slider: 1-100]

"Average file complexity?"
[Dropdown: Simple / Medium / Complex / Very Complex]

"Estimated monthly cost: $X"
"Recommended tier: Professional"
```

---

## 5. Psychological Pricing Tactics

### Anchor Pricing
Position Professional tier as the default choice:

```
┌─────────────┐   ┌──────────────────┐   ┌─────────────┐
│   Starter   │   │  PROFESSIONAL ⭐  │   │   Studio    │
│             │   │   MOST POPULAR   │   │             │
│    $25      │   │      $50         │   │    $200     │
│  50 credits │   │   110 credits    │   │ 500 credits │
└─────────────┘   └──────────────────┘   └─────────────┘
                           ↑
                    Visual emphasis
```

### Charm Pricing (Optional)
Consider testing these alternatives:
- $49 instead of $50 (Professional)
- $199 instead of $200 (Studio)
- $24 instead of $25 (Starter)

**Recommendation:** Keep round numbers for now (easier mental math), test charm pricing after 3 months.

### Decoy Pricing
Professional tier is the "decoy" that makes Studio look like better value:

```
Professional: $50 for 110 credits = $0.45/credit
Studio: $200 for 500 credits = $0.40/credit (11% cheaper!)
```

Many customers will upgrade to Studio when they see the math.

---

## 6. Discount & Promotion Strategy

### Launch Promotion (First 30 Days)
**"Early Adopter Discount"**
- 30% off first purchase (any tier)
- Discount code: `LAUNCH30`
- Limited to first 100 customers
- Creates urgency and rewards early adopters

### Ongoing Promotions

#### Seasonal Sales
- **Black Friday:** 40% off all tiers
- **New Year:** 25% off (Q1 budget refresh)
- **Back to School:** 20% off (August-September)

#### Referral Program
```
Referrer: 20% commission on referred customer's first purchase
Referee: 15% off first purchase
```

**Example:**
- Alice refers Bob
- Bob purchases $200 Studio tier
- Alice receives $40 credit
- Bob pays $170 (15% off)

#### Volume Discounts (Auto-Applied)
```
Lifetime Credits Purchased | Discount on Next Purchase
---------------------------|-------------------------
1,000+                     | 5% off
2,500+                     | 10% off
5,000+                     | 15% off
10,000+                    | Contact for Enterprise pricing
```

### Abandoned Cart Recovery
If user adds credits to cart but doesn't complete purchase:
```
+1 hour: "You left credits in your cart" (reminder)
+24 hours: "Still interested? Here's 10% off" (incentive)
+72 hours: "Last chance: 15% off expires in 24h" (urgency)
```

**Expected Recovery:** 15-25% of abandoned carts

---

## 7. Enterprise Pricing Strategy

### Custom Quote Framework

**Base Pricing Model:**
```
Credits/Month    | Cost per Credit | Monthly Price  | Annual Price
-----------------|-----------------|----------------|---------------
3,750 (750/mo)   | $0.40          | $1,500         | $15,000 (save 17%)
7,500 (1,500/mo) | $0.35          | $2,625         | $27,000 (save 17%)
12,500+          | $0.30          | $3,750+        | $38,000+ (save 17%)
```

### Enterprise Add-Ons (À La Carte)
- **Priority Queue:** +$300/month (guaranteed <5min processing)
- **Dedicated Support:** +$500/month (Slack channel, 2h response SLA)
- **Custom Integration:** $2,000-$5,000 one-time
- **White-Label API:** +$1,000/month
- **On-Premise Deployment:** $10,000+ (future offering)

### Negotiation Guidelines

**Discounting Authority:**
- Sales rep: Up to 10% discount
- Founder approval: 10-25% discount
- Board approval: 25%+ discount (rare, strategic deals only)

**Non-Negotiables:**
- Minimum 6-month contract
- Payment terms: Net 30 maximum
- No free trials beyond standard free tier

---

## 8. Competitive Positioning

### Price Comparison Matrix

Create this table for sales materials:

| Provider | 100 Files/Month | 5GB File Support | API Access | Normal Baking | USDZ Output |
|----------|----------------|------------------|------------|---------------|-------------|
| **Your Service** | **$200** | ✅ Yes | ✅ Yes | ✅ Yes | ✅ Yes |
| Meshy.ai | $300 | ❌ No (500MB) | ✅ Yes | ✅ Yes | ⚠️ Limited |
| RapidPipeline | $768/year | ⚠️ 2GB | ✅ Yes | ❌ No | ❌ No |
| Meshmatic | $600/year | ✅ Yes | ❌ No | ⚠️ Limited | ❌ No |

**Value Proposition:**
> "Only mesh optimizer that handles 5GB files with complete remeshing, normal baking, and dual GLB/USDZ output — all through a simple API."

---

## 9. Implementation Timeline

### Week 1: Preparation
- [ ] Update codebase with tiered pricing logic
- [ ] Create Stripe products for each tier
- [ ] Design pricing page with tier comparison
- [ ] Write FAQ about credit usage

### Week 2: Free Tier Launch
- [ ] Implement free tier (25 credits on signup)
- [ ] Set up email automation (welcome sequence)
- [ ] Create usage tracking analytics
- [ ] Test conversion funnel

### Week 3: Paid Tiers Launch
- [ ] Enable Starter, Professional, Studio tiers
- [ ] Launch "Early Adopter" promotion
- [ ] Publish credit usage calculator
- [ ] Begin content marketing campaign

### Week 4: Enterprise Outreach
- [ ] Create enterprise sales deck
- [ ] Compile target enterprise prospect list (50 companies)
- [ ] Launch LinkedIn outreach campaign
- [ ] Schedule first 10 sales calls

### Month 2-3: Optimization
- [ ] A/B test pricing ($49 vs $50)
- [ ] Analyze tier distribution (adjust if needed)
- [ ] Implement referral program
- [ ] Launch first case study

---

## 10. Key Metrics & Targets

### Conversion Metrics
| Metric | Target | Measurement |
|--------|--------|-------------|
| **Free → Paid Conversion** | 20% in 30 days | Track cohort analysis |
| **Starter → Professional Upgrade** | 15% in 90 days | Track upgrade rate |
| **Cart Abandonment Rate** | <30% | Monitor checkout flow |
| **Enterprise Close Rate** | 10% of qualified leads | CRM pipeline tracking |

### Revenue Metrics
| Metric | Month 1 | Month 3 | Month 6 | Month 12 |
|--------|---------|---------|---------|----------|
| **MRR** | $2,500 | $10,000 | $25,000 | $50,000+ |
| **ARPU** | $250 | $300 | $350 | $400 |
| **Enterprise Revenue** | $0 | $0 | $3,000 | $15,000 |

### Tier Distribution Targets
```
Free Tier: 60% of signups (conversion source)
Starter: 20% of paid customers
Professional: 50% of paid customers ⭐ (highest volume)
Studio: 25% of paid customers
Enterprise: 5% of paid customers (40%+ of revenue)
```

---

## 11. Pricing FAQ (For Sales/Support)

**Q: Why is your pricing higher than simple decimation services?**
A: We offer a complete optimization pipeline (remeshing, normal baking, dual format output) and support 5GB files — 10x larger than competitors. You're paying for quality and capability.

**Q: Do credits expire?**
A: No! Credits never expire. Buy them once, use them whenever you need.

**Q: Can I upgrade/downgrade my plan?**
A: There are no "plans" — just credit purchases. Buy the amount you need, when you need it. Enterprise customers have monthly commitments.

**Q: What happens if I run out of credits mid-job?**
A: Jobs are charged only on completion. If you don't have enough credits, the job will fail gracefully, and we'll notify you to purchase more.

**Q: Is there a refund policy?**
A: We offer a 7-day money-back guarantee if you're unsatisfied with the service. Used credits will be deducted from the refund at $0.50/credit.

**Q: Can I get a discount for educational use?**
A: Yes! Email us with your .edu address for a 30% education discount on all purchases.

---

## 12. Recommendations Summary

### Immediate Actions (This Week)
1. ✅ **Implement Free Tier:** 25 credits on signup
2. ✅ **Launch 4 Pricing Tiers:** Free, Starter ($25), Professional ($50), Studio ($200)
3. ✅ **Create Pricing Calculator:** Help users estimate costs
4. ✅ **Design Comparison Table:** Highlight value vs competitors

### Short-Term (30 Days)
1. Monitor tier distribution (adjust pricing if needed)
2. A/B test pricing page layout
3. Launch referral program
4. Begin enterprise outreach

### Medium-Term (90 Days)
1. Analyze customer behavior by tier
2. Optimize credit consumption per job type
3. Launch volume discount program
4. Publish 3 case studies

### Long-Term (6-12 Months)
1. Consider subscription option (monthly credits)
2. Expand enterprise features (white-label, SSO)
3. Dynamic pricing based on demand
4. Partner/reseller program

---

## 13. Final Pricing Model

### Recommended Launch Configuration

```env
# Free Tier
FREE_TIER_CREDITS=25

# Paid Tiers
STARTER_PRICE=2500      # $25 in cents
STARTER_CREDITS=50

PROFESSIONAL_PRICE=5000  # $50 in cents
PROFESSIONAL_CREDITS=110

STUDIO_PRICE=20000       # $200 in cents
STUDIO_CREDITS=500

# Enterprise (custom quotes)
ENTERPRISE_MIN_PRICE=150000  # $1,500/month minimum
```

### Stripe Product IDs (To Be Created)
```
prod_starter_50_credits
prod_professional_110_credits
prod_studio_500_credits
```

---

**This pricing strategy balances accessibility (free + $25 entry) with revenue optimization (volume discounts, enterprise tiers). The psychological positioning makes Professional tier the natural choice, while Studio tier rewards loyal customers. Execute this strategy, and you'll maximize both customer acquisition and lifetime value.**

---

*Document Version 1.0 | Approved for Implementation | Review quarterly*