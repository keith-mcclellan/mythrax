from apscheduler.schedulers.background import BackgroundScheduler
from mythrax_forge.synthesis import DreamCoordinator
from typing import Any
import logging

logger = logging.getLogger("mythrax.scheduler")

class ForgeScheduler:
    def __init__(self, client: Any):
        self.client = client
        self.scheduler = BackgroundScheduler()
        self.coordinator = DreamCoordinator(client)

    def trigger_dream_job(self):
        logger.info("Executing scheduled dreaming compaction run...")
        try:
            rules = self.coordinator.run_synthesis_dream()
            logger.info(f"Dreaming job completed. Generated {len(rules)} synthesized rules.")
        except Exception as e:
            logger.error(f"Scheduled dreaming job failed: {e}")

    def start(self):
        # Run dreaming cycles every hour
        self.scheduler.add_job(self.trigger_dream_job, 'interval', hours=1)
        self.scheduler.start()
        logger.info("Forge background scheduler started successfully.")

    def stop(self):
        self.scheduler.shutdown()
        logger.info("Forge background scheduler stopped.")
